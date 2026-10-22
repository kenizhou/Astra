// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the send gate at the submit seam: every send a submit starts
//! passes the installed transform and gate before it queues, and a blocked or
//! unanswered send stands as a held row naming the plugin, never queued,
//! never silently sent.

use std::sync::{Arc, Mutex};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use astra_viewmodel::HeldState;
use engine_api::AccountId;

use crate::{
    dispatch_fakes as fakes, dispatch_fakes::ScriptedSend, App, BeforeSendGate, ComposeTransform,
    GateComposition, GateVerdict, Intent, OutboxIntent, Surface, TransformFailure,
};

/// A well-formed account id, the way the gate tests name one.
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

/// The composer's Send button on a plain message about `text`.
fn send(text: &str) -> Intent {
    Intent::SubmitRichMail {
        from: account("work"),
        to: "ada@example.test".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "Quarterly report".to_owned(),
        document: document(text),
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    }
}

/// A scripted gate: one canned verdict per check, in order, then the last
/// forever.
struct ScriptedGate {
    verdicts: Mutex<Vec<GateVerdict>>,
    seen: Mutex<Vec<String>>,
}

impl ScriptedGate {
    fn answering(verdicts: Vec<GateVerdict>) -> Self {
        Self {
            verdicts: Mutex::new(verdicts),
            seen: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl BeforeSendGate for ScriptedGate {
    async fn check(&self, composition: &GateComposition) -> GateVerdict {
        self.seen.lock().unwrap().push(composition.subject.clone());
        let mut verdicts = self.verdicts.lock().unwrap();
        if verdicts.len() > 1 {
            verdicts.remove(0)
        } else {
            verdicts[0].clone()
        }
    }
}

/// A scripted transform: appends a marker to both legs, or fails when
/// configured to.
struct ScriptedTransform {
    fail: bool,
    seen: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl ComposeTransform for ScriptedTransform {
    async fn transform(
        &self,
        mut composition: GateComposition,
    ) -> Result<GateComposition, TransformFailure> {
        self.seen.lock().unwrap().push(composition.plain.clone());
        if self.fail {
            return Err(TransformFailure {
                plugin: "dev.astra.signer".to_owned(),
                reason: "no signing key is configured".to_owned(),
            });
        }
        composition.plain.push_str("\n-- signed");
        composition.html.push_str("<!-- signed -->");
        Ok(composition)
    }
}

/// An in-memory app over one `work` account whose SMTP leg answers `script`,
/// with the given gate and transform installed.
async fn app_with(
    script: Vec<ScriptedSend>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
    gate: Option<Arc<dyn BeforeSendGate>>,
    transform: Option<Arc<dyn ComposeTransform>>,
) -> (App<fakes::FakeProvider>, Arc<Mutex<Vec<engine_api::Draft>>>) {
    let provider = fakes::FakeProvider::with_mail(vec![fakes::inbox()], vec![]).submitting(script);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces)
        .with_send_gate(gate, transform);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    (app, submits)
}

#[tokio::test]
async fn an_allowing_gate_lets_the_send_queue_as_today() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(
        vec![ScriptedSend::Delivered],
        &surfaces,
        Some(Arc::new(ScriptedGate::answering(vec![GateVerdict::Allow]))),
        None,
    )
    .await;

    app.dispatch(send("Agreed")).await.unwrap();

    assert_eq!(submits.lock().unwrap().len(), 1, "the send went out");
    assert!(app.sending().held.is_empty(), "nothing holds");
}

#[tokio::test]
async fn a_blocked_send_is_never_queued_and_stands_held_naming_the_plugin() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(
        vec![ScriptedSend::Delivered],
        &surfaces,
        Some(Arc::new(ScriptedGate::answering(vec![
            GateVerdict::Block {
                plugin: "dev.astra.crypto".to_owned(),
                reason: "no ready key for ada@example.test".to_owned(),
            },
        ]))),
        None,
    )
    .await;

    app.dispatch(send("Agreed")).await.unwrap();

    assert_eq!(
        submits.lock().unwrap().len(),
        0,
        "a blocked send never reaches the transport"
    );
    let sending = app.sending();
    assert!(sending.rows.is_empty(), "and it is never queued");
    assert_eq!(sending.held.len(), 1, "it stands held");
    let held = &sending.held[0];
    assert_eq!(held.state, HeldState::Blocked);
    assert_eq!(held.plugin, "dev.astra.crypto");
    assert_eq!(held.reason, "no ready key for ada@example.test");
    assert_eq!(held.subject, "Quarterly report");
}

#[tokio::test]
async fn a_prompt_send_anyway_queues_and_a_deny_drops_the_hold() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(
        vec![ScriptedSend::Delivered, ScriptedSend::Delivered],
        &surfaces,
        Some(Arc::new(ScriptedGate::answering(vec![
            GateVerdict::PromptUser {
                plugin: "dev.astra.dlp".to_owned(),
                reason: "this names an external recipient".to_owned(),
            },
        ]))),
        None,
    )
    .await;

    app.dispatch(send("First")).await.unwrap();
    let held = &app.sending().held[0];
    assert_eq!(held.state, HeldState::NeedsAnswer);
    assert_eq!(held.plugin, "dev.astra.dlp");

    // Deny: the hold drops, nothing sends.
    app.dispatch(Intent::Outbox(OutboxIntent::AnswerGate {
        hold: held.hold.clone(),
        send_anyway: false,
    }))
    .await
    .unwrap();
    assert!(app.sending().held.is_empty(), "the denied prompt is gone");
    assert_eq!(submits.lock().unwrap().len(), 0);

    // Again, this time Send Anyway: the send queues without re-gating.
    app.dispatch(send("Second")).await.unwrap();
    let held = &app.sending().held[0];
    app.dispatch(Intent::Outbox(OutboxIntent::AnswerGate {
        hold: held.hold.clone(),
        send_anyway: true,
    }))
    .await
    .unwrap();
    assert_eq!(submits.lock().unwrap().len(), 1, "Send Anyway queued it");
    assert!(app.sending().held.is_empty());
}

#[tokio::test]
async fn a_retry_re_gates_the_held_send() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let gate = Arc::new(ScriptedGate::answering(vec![
        GateVerdict::Block {
            plugin: "dev.astra.crypto".to_owned(),
            reason: "worker unavailable".to_owned(),
        },
        GateVerdict::Allow,
    ]));
    let (app, submits) = app_with(
        vec![ScriptedSend::Delivered],
        &surfaces,
        Some(gate.clone()),
        None,
    )
    .await;

    app.dispatch(send("Agreed")).await.unwrap();
    let hold = app.sending().held[0].hold.clone();
    assert_eq!(submits.lock().unwrap().len(), 0);

    app.dispatch(Intent::Outbox(OutboxIntent::RetryHeld { hold }))
        .await
        .unwrap();

    assert_eq!(submits.lock().unwrap().len(), 1, "the retry went out");
    assert!(app.sending().held.is_empty());
    assert_eq!(
        gate.seen.lock().unwrap().len(),
        2,
        "the retry ran the gate again"
    );
}

#[tokio::test]
async fn a_withdraw_drops_the_hold_and_keeps_the_draft_unsent() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(
        vec![ScriptedSend::Delivered],
        &surfaces,
        Some(Arc::new(ScriptedGate::answering(vec![
            GateVerdict::Block {
                plugin: "dev.astra.crypto".to_owned(),
                reason: "untrusted recipient".to_owned(),
            },
        ]))),
        None,
    )
    .await;

    app.dispatch(send("Agreed")).await.unwrap();
    let hold = app.sending().held[0].hold.clone();
    app.dispatch(Intent::Outbox(OutboxIntent::WithdrawHeld { hold }))
        .await
        .unwrap();

    assert!(app.sending().held.is_empty(), "the hold is gone");
    assert_eq!(submits.lock().unwrap().len(), 0, "nothing ever sent");
}

#[tokio::test]
async fn the_transform_rewrites_both_legs_before_the_gate_checks() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let transform = Arc::new(ScriptedTransform {
        fail: false,
        seen: Mutex::new(Vec::new()),
    });
    let (app, submits) = app_with(
        vec![ScriptedSend::Delivered],
        &surfaces,
        Some(Arc::new(ScriptedGate::answering(vec![GateVerdict::Allow]))),
        Some(transform.clone()),
    )
    .await;

    app.dispatch(send("Agreed")).await.unwrap();

    let draft = &submits.lock().unwrap()[0];
    assert!(
        draft.text_body.ends_with("\n-- signed"),
        "the plain leg carries the transform: {:?}",
        draft.text_body
    );
    assert!(
        draft
            .html_body
            .as_deref()
            .unwrap_or_default()
            .ends_with("<!-- signed -->"),
        "the HTML leg carries it too"
    );
    assert_eq!(transform.seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_failing_transform_holds_the_send_closed() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(
        vec![ScriptedSend::Delivered],
        &surfaces,
        Some(Arc::new(ScriptedGate::answering(vec![GateVerdict::Allow]))),
        Some(Arc::new(ScriptedTransform {
            fail: true,
            seen: Mutex::new(Vec::new()),
        })),
    )
    .await;

    app.dispatch(send("Agreed")).await.unwrap();

    assert_eq!(
        submits.lock().unwrap().len(),
        0,
        "the untransformed mail never sends"
    );
    let held = &app.sending().held[0];
    assert_eq!(held.state, HeldState::Blocked);
    assert_eq!(held.plugin, "dev.astra.signer");
    assert_eq!(held.reason, "no signing key is configured");
}

#[tokio::test]
async fn without_a_gate_installed_the_submit_costs_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(vec![ScriptedSend::Delivered], &surfaces, None, None).await;

    app.dispatch(send("Agreed")).await.unwrap();

    assert_eq!(submits.lock().unwrap().len(), 1);
    assert!(app.sending().held.is_empty());
}
