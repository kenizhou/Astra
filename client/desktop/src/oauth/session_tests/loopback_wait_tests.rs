// SPDX-License-Identifier: GPL-3.0-only
//! The displaced loopback waiter: the claim-then-cancel ordering and the
//! never-requeued capture. Split from the session tests for the
//! 500-line cap.

use super::*;

#[test]
fn a_displaced_loopback_waiter_answers_cancelled_and_never_requeues() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let browser = CountingBrowser::opening();
            let (_deep_tx, deep_rx) = mpsc::unbounded_channel();
            let session = std::sync::Arc::new(SignInSession::new(
                as_seam(&browser),
                deep_rx,
                mpsc::unbounded_channel().0,
            ));

            let (capture_tx, capture_rx) = mpsc::channel(1);
            let displaced = session
                .begin_sign_in(&client(), None, CaptureSource::Loopback(capture_rx))
                .expect("the browser opened for the first flow");
            // The waiter claims its flow's receiver as it starts waiting
            // (the spawned half's shape), so the cancel that follows takes
            // the pair back while the claimed receiver stays open for the
            // late capture. The claim is the ordering under test: wait
            // until the spawned waiter has taken its flow's receiver
            // before the cancel runs, or the cancel would race the
            // unclaimed channel drop (the production waiter claims within
            // microseconds of begin; a human's cancel never beats it).
            let waiter = tokio::spawn({
                let session = std::sync::Arc::clone(&session);
                async move { session.wait_for_callback(displaced).await }
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !session.loopback_claimed() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the waiter never claimed its capture"
                );
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert!(session.cancel_sign_in(), "the first flow is taken back");
            let standing = session
                .begin_sign_in(&client(), None, CaptureSource::DeepLink)
                .expect("the second flow stands");

            // The displaced flow's listener captures late; its waiter
            // answers the cancellation, and the capture is dropped, not
            // requeued: a capture on this flow's own port belongs to no
            // other flow, and the standing deep-link flow receives
            // nothing from it.
            capture_tx
                .send(LoopbackCapture::Received(LOOPBACK_CAPTURE.to_owned()))
                .await
                .expect("the late capture crosses");
            let answered = waiter
                .await
                .expect("the waiter task ends")
                .expect_err("the displaced waiter answers the cancellation");
            assert!(
                matches!(answered, SignInError::Cancelled),
                "the cancellation is the answer: {answered}"
            );
            let standing_saw_nothing = tokio::time::timeout(
                std::time::Duration::from_millis(150),
                session.wait_for_callback(standing),
            )
            .await
            .is_err();
            assert!(
                standing_saw_nothing,
                "a loopback capture never crosses into the deep-link channel"
            );
        });
}
