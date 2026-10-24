// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the notification bridge: the focus rules, the
//! three-then-summary bound, the kill switch, and the shape of one
//! message's notification, all over a recording sink so no OS is involved.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use astra_app::{NewMailBatch, NewMailNotice};
use engine_api::{AccountId, EmailAddress, ProviderKey};

use super::{NotifyBridge, OsNotifier};

/// Every notification the sink was asked to show.
#[derive(Default)]
struct Recording {
    shown: Mutex<Vec<(String, String)>>,
}

/// The sink the bridge holds a second handle to, so the test reads what
/// crossed.
#[derive(Clone, Default)]
struct SharedRecording(Arc<Recording>);

impl OsNotifier for SharedRecording {
    fn show(&self, title: &str, body: &str) {
        self.0
            .shown
            .lock()
            .unwrap()
            .push((title.to_owned(), body.to_owned()));
    }
}

/// A bridge over a recording sink, its focus state held by the test.
fn bridge(focused: bool) -> (NotifyBridge, Arc<AtomicBool>, Arc<Recording>) {
    let focus = Arc::new(AtomicBool::new(focused));
    let probe: Arc<dyn Fn() -> bool + Send + Sync> = {
        let focus = Arc::clone(&focus);
        Arc::new(move || focus.load(Ordering::SeqCst))
    };
    let sink = SharedRecording::default();
    let record = Arc::clone(&sink.0);
    (NotifyBridge::new(Box::new(sink), probe), focus, record)
}

/// One notice, minted around `n`.
fn notice(n: usize) -> NewMailNotice {
    NewMailNotice {
        account: AccountId::try_from("work").unwrap(),
        key: ProviderKey::new(format!("k{n}")).unwrap(),
        from: Some(EmailAddress::named(
            format!("Sender {n}"),
            format!("sender{n}@example.test"),
        )),
        subject: Some(format!("Subject {n}")),
        preview: Some(format!("Preview {n}")),
    }
}

/// A batch of `count` messages, no overflow.
fn batch(count: usize) -> NewMailBatch {
    NewMailBatch {
        messages: (0..count).map(notice).collect(),
        overflow: 0,
    }
}

/// What the sink has shown so far.
fn shown(record: &Arc<Recording>) -> Vec<(String, String)> {
    record.shown.lock().unwrap().clone()
}

#[test]
fn one_notification_per_message_up_to_three_while_unfocused() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver(&batch(3), true);
    let shown = shown(&record);
    assert_eq!(shown.len(), 3, "one per message: {shown:?}");
    assert_eq!(
        shown[0],
        ("Sender 0".to_owned(), "Subject 0\nPreview 0".to_owned())
    );
}

#[test]
fn a_single_summary_replaces_the_notifications_past_three() {
    let (bridge, _focus, record) = bridge(false);
    let mut folded = batch(3);
    folded.overflow = 2;
    bridge.deliver(&folded, true);
    let shown = shown(&record);
    assert_eq!(shown.len(), 1, "one summary, not a storm: {shown:?}");
    assert_eq!(
        shown[0],
        ("New mail".to_owned(), "5 new messages".to_owned()),
        "the summary counts the overflow the queue folded away"
    );
}

#[test]
fn nothing_while_the_window_is_focused() {
    // The person is already looking at the list; a notification would be
    // noise beside it.
    let (bridge, focus, record) = bridge(true);
    bridge.deliver(&batch(2), true);
    assert!(shown(&record).is_empty());
    focus.store(false, Ordering::SeqCst);
    bridge.deliver(&batch(2), true);
    assert_eq!(shown(&record).len(), 2, "the next unfocused pass announces");
}

#[test]
fn nothing_while_the_switch_is_off() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver(&batch(2), false);
    assert!(
        shown(&record).is_empty(),
        "the kill switch beats every other rule"
    );
}

#[test]
fn nothing_for_an_empty_batch() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver(&NewMailBatch::default(), true);
    assert!(shown(&record).is_empty());
}

#[test]
fn a_message_without_a_sender_or_subject_still_announces_itself() {
    // What the envelope omitted the notification falls back past: an
    // address-less From, no subject, no preview, and the notification is
    // still worth raising.
    let (bridge, _focus, record) = bridge(false);
    let bare = NewMailNotice {
        account: AccountId::try_from("work").unwrap(),
        key: ProviderKey::new("k").unwrap(),
        from: None,
        subject: None,
        preview: None,
    };
    bridge.deliver(
        &NewMailBatch {
            messages: vec![bare],
            overflow: 0,
        },
        true,
    );
    let shown = shown(&record);
    assert_eq!(shown.len(), 1);
    assert_eq!(
        shown[0],
        ("New mail".to_owned(), "You have a new message".to_owned())
    );
}
