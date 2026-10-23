// SPDX-License-Identifier: GPL-3.0-only
//! The crypto flags' journey (ECS): `submit_and_drain` stamps the resolved
//! toggles onto the engine's `Draft`, an explicit composer toggle is
//! respected, and a send the plugin gate held resumes with its flags intact.
//! What the flags DO: wrap, or refuse: is the engine's seam and the
//! store's wrapper, proven in their own suites; here the proof is that the
//! app hands the flags over faithfully.

use std::sync::{Arc, Mutex};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use engine_api::AccountId;

use super::CryptoFlags;
use crate::dispatch_fakes::{self as fakes, ScriptedSend};
use crate::gate::{BeforeSendGate, GateComposition, GateVerdict};
use crate::{Intent, OutboxIntent, Surface};

/// A gate that asks once (the send stands held awaiting the answer), then
/// allows.
struct AskingGate;

#[async_trait::async_trait]
impl BeforeSendGate for AskingGate {
    async fn check(&self, _composition: &GateComposition) -> GateVerdict {
        GateVerdict::PromptUser {
            plugin: "dev.astra.asker".to_owned(),
            reason: "send this?".to_owned(),
        }
    }
}

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// A one-paragraph document carrying `text`, the smallest document a send holds.
fn document(text: &str) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![Block::Paragraph(Paragraph {
            content: vec![InlineContent::Text(TextRun {
                text: text.to_owned(),
                bold: false,
                italic: false,
                underline: false,
                font_size: None,
                color: None,
                highlight: None,
                link: None,
            })],
        })],
        attachments: Vec::new(),
    }
}

/// The composer's Send button with explicit crypto toggles.
fn send(text: &str, crypto: CryptoFlags) -> Intent {
    Intent::SubmitRichMail {
        from: account("work"),
        to: "ada@example.test".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "Quarterly report".to_owned(),
        document: document(text),
        blobs: Vec::new(),
        crypto,
    }
}

/// An in-memory app over one `work` account whose SMTP leg delivers, with
/// the given gate installed.
async fn app_with(
    surfaces: &Arc<Mutex<Vec<Surface>>>,
    gate: Option<Arc<dyn crate::gate::BeforeSendGate>>,
) -> (crate::App<fakes::FakeProvider>, Arc<Mutex<Vec<engine_api::Draft>>>) {
    let provider =
        fakes::FakeProvider::with_mail(vec![fakes::inbox()], vec![]).submitting(vec![
            ScriptedSend::Delivered,
            ScriptedSend::Delivered,
        ]);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces).with_send_gate(gate, None);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    (app, submits)
}

#[tokio::test]
async fn the_default_flags_stamp_sign_and_encrypt_onto_the_engine_draft() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(&surfaces, None).await;

    app.dispatch(send("Agreed", CryptoFlags::default()))
        .await
        .unwrap();

    let submits = submits.lock().unwrap();
    assert_eq!(submits.len(), 1, "the send went out");
    assert!(submits[0].sign, "the default signs under a crypto method");
    assert!(submits[0].encrypt, "the default encrypts under a crypto method");
}

#[tokio::test]
async fn an_explicit_toggle_is_respected() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(&surfaces, None).await;

    app.dispatch(send(
        "Signed only",
        CryptoFlags {
            sign: Some(true),
            encrypt: Some(false),
        },
    ))
    .await
    .unwrap();
    app.dispatch(send(
        "Sealed only",
        CryptoFlags {
            sign: Some(false),
            encrypt: Some(true),
        },
    ))
    .await
    .unwrap();

    let submits = submits.lock().unwrap();
    assert_eq!(submits.len(), 2);
    assert!(submits[0].sign && !submits[0].encrypt, "the first send's toggles");
    assert!(!submits[1].sign && submits[1].encrypt, "the second send's toggles");
}

#[tokio::test]
async fn a_held_send_resumes_with_its_flags_intact() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(
        &surfaces,
        Some(Arc::new(AskingGate)),
    )
    .await;

    // Non-default toggles, so the resume has something to prove.
    app.dispatch(send(
        "Held then sent",
        CryptoFlags {
            sign: Some(false),
            encrypt: Some(true),
        },
    ))
    .await
    .unwrap();
    let held = app.sending().held;
    assert_eq!(held.len(), 1, "the gate's question holds the send");
    assert!(submits.lock().unwrap().is_empty(), "nothing went out yet");

    app.dispatch(Intent::Outbox(OutboxIntent::AnswerGate {
        hold: held[0].hold.clone(),
        send_anyway: true,
    }))
    .await
    .unwrap();

    let submits = submits.lock().unwrap();
    assert_eq!(submits.len(), 1, "Send Anyway resumed the send");
    assert!(
        !submits[0].sign && submits[0].encrypt,
        "the resumed send carries the toggles the composer set"
    );
}
