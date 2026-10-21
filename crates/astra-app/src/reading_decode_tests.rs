// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the decode seam in the reading pipeline: when a decode-serving
//! plugin answers, its parts replace the rendered legs (the decoded HTML
//! through the identical sanitise pipeline), its banner rides the snapshot,
//! and an absent or failing decoder changes nothing.

use std::sync::{Arc, Mutex};

use astra_viewmodel::{SecurityBannerView, SecurityLevelView};

use crate::{
    dispatch_fakes as fakes, App, DecodeOutcome, Intent, MessageDecoder, MessageRef, Surface,
};

/// A scripted decoder: answers the canned outcome, recording the raw it saw.
struct ScriptedDecoder {
    outcome: Option<DecodeOutcome>,
    seen: Mutex<Vec<usize>>,
}

#[async_trait::async_trait]
impl MessageDecoder for ScriptedDecoder {
    async fn decode(&self, message: &MessageRef, raw: &[u8]) -> Option<DecodeOutcome> {
        let _ = message;
        self.seen.lock().unwrap().push(raw.len());
        self.outcome.clone()
    }
}

/// The fixture message's raw source: a plain leg carrying the words the
/// untouched-rendering assertions read back.
fn source() -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(b"Content-Type: text/plain; charset=\"utf-8\"\r\n\r\n");
    raw.extend_from_slice(b"The words here.\r\n");
    raw
}

/// An in-memory app over one `work` account holding one message, synced, with
/// the decoder installed.
async fn app_decoding(decoder: Option<Arc<dyn MessageDecoder>>) -> App<fakes::FakeProvider> {
    let surfaces = Arc::new(Mutex::new(Vec::<Surface>::new()));
    let message = fakes::mail("m1", "inbox", "Quarterly report");
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![message],
        vec![("m1".to_owned(), source())],
    );
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces).with_decoder(decoder);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// Opens the fixture message and answers the reading snapshot.
async fn open(app: &App<fakes::FakeProvider>) -> astra_viewmodel::ReadingSnapshot {
    app.dispatch(Intent::OpenMessage {
        message: MessageRef::from_parts("work", "m1".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    app.reading()
        .expect("the open publishes a reading snapshot")
}

#[tokio::test]
async fn a_decode_replaces_both_legs_and_the_banner_rides() {
    let decoder = Arc::new(ScriptedDecoder {
        outcome: Some(DecodeOutcome {
            html: Some("<p>decrypted words</p>".to_owned()),
            plain: Some("decrypted words".to_owned()),
            banner: Some(SecurityBannerView {
                level: SecurityLevelView::Warning,
                text: "Signature invalid".to_owned(),
            }),
        }),
        seen: Mutex::new(Vec::new()),
    });
    let app = app_decoding(Some(decoder.clone())).await;

    let reading = open(&app).await;

    assert_eq!(reading.plain.as_deref(), Some("decrypted words"));
    assert!(
        reading
            .html
            .as_deref()
            .is_some_and(|html| html.contains("decrypted words")),
        "the decoded HTML renders: {:?}",
        reading.html
    );
    assert_eq!(
        reading.security_banner,
        Some(SecurityBannerView {
            level: SecurityLevelView::Warning,
            text: "Signature invalid".to_owned(),
        }),
        "the banner rides the snapshot"
    );
    assert_eq!(
        decoder.seen.lock().unwrap().len(),
        1,
        "the decoder saw the raw source exactly once"
    );
}

#[tokio::test]
async fn decoded_html_takes_the_same_sanitise_pipeline_as_any_body() {
    let decoder = Arc::new(ScriptedDecoder {
        outcome: Some(DecodeOutcome {
            html: Some(r#"<p onclick="steal()">safe words</p><script>steal()</script>"#.to_owned()),
            plain: None,
            banner: None,
        }),
        seen: Mutex::new(Vec::new()),
    });
    let app = app_decoding(Some(decoder)).await;

    let reading = open(&app).await;
    let html = reading.html.as_deref().expect("the decoded html");
    assert!(html.contains("safe words"), "the content survives");
    assert!(!html.contains("script"), "the script tag is gone: {html}");
    assert!(!html.contains("steal"), "the handler is gone: {html}");
}

#[tokio::test]
async fn an_unanswered_decode_leaves_the_rendering_untouched() {
    let decoder = Arc::new(ScriptedDecoder {
        outcome: None,
        seen: Mutex::new(Vec::new()),
    });
    let app = app_decoding(Some(decoder.clone())).await;

    let reading = open(&app).await;

    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains("The words here.")),
        "the store's own body renders: {:?}",
        reading.plain
    );
    assert_eq!(reading.security_banner, None, "no banner without an answer");
    assert_eq!(decoder.seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn without_a_decoder_the_reading_costs_nothing() {
    let app = app_decoding(None).await;
    let reading = open(&app).await;
    assert!(reading.plain.is_some());
    assert_eq!(reading.security_banner, None);
}
