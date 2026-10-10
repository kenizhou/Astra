// SPDX-License-Identifier: GPL-3.0-only
//! Host-pull tests for the reply-drafting model: the chips read runs the
//! gate over the pool and honours the dismissal marker, the summarise
//! read answers cached and errors on a message it cannot resolve, the
//! style read infers from Sent mail and answers none without it, and an
//! app with no AI bundle fails every pull quiet. A child of the driver
//! tests, sharing their fixtures.

use std::sync::{Arc, Mutex};

use astra_ai::{AiStore, CorrelationId, NewDraftPoolRow};

use super::{drafted_app, message, source, DRAFT, STYLE};
use crate::ai_replies::SummariseMessageError;
use crate::dispatch_fakes as fakes;

/// Seeds one drafted pool row for `key` at medium confidence, the row the
/// chips gate clears.
fn seed_draft(store: &AiStore, key: &str) {
    store
        .insert_draft_pool(&NewDraftPoolRow {
            correlation: CorrelationId::generate(),
            message_account: "work".to_owned(),
            message_key: key.to_owned(),
            reply: Some("Sounds good, shipped it.".to_owned()),
            confidence: Some(0.6),
        })
        .expect("the seed row lands");
}

#[tokio::test]
async fn the_chips_pull_reads_the_gate_and_a_dismissal_hides_the_chips() {
    let complete = fakes::ai::FixtureComplete::answering(DRAFT);
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    seed_draft(&store, "m3");
    let app = drafted_app(super::thread(), complete, Arc::clone(&store), None).await;

    let chips = app
        .draft_chips(&message("m3").account, &message("m3").key)
        .await;
    assert_eq!(
        chips,
        vec![crate::Chip {
            reply: "Sounds good, shipped it.".to_owned(),
            preview: "Sounds good, shipped it.".to_owned(),
        }],
        "the pull ran the gate over the pooled row",
    );

    app.dismiss_chips(&message("m3").account, &message("m3").key);
    assert!(
        app.draft_chips(&message("m3").account, &message("m3").key)
            .await
            .is_empty(),
        "the dismissal marker hides the message's chips",
    );
    assert!(
        store
            .chips_dismissed("work", "m3")
            .expect("the marker read"),
        "the dismiss write landed in the store",
    );
}

#[tokio::test]
async fn the_summarise_pull_answers_cached_and_errors_on_an_unresolvable_message() {
    let complete = fakes::ai::FixtureComplete::answering(r#"{"summary":"The launch is ready."}"#);
    let probe = complete.clone();
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = drafted_app(super::thread(), complete, Arc::clone(&store), None).await;

    let first = app
        .summarise_message(&message("m3").account, &message("m3").key)
        .await
        .expect("the summarise answers");
    assert_eq!(first, "The launch is ready.");
    let second = app
        .summarise_message(&message("m3").account, &message("m3").key)
        .await
        .expect("the second pull answers");
    assert_eq!(second, "The launch is ready.");
    assert_eq!(
        probe.call_count(),
        1,
        "the cached second pull spent no second call",
    );

    let missing = app
        .summarise_message(&message("nope").account, &message("nope").key)
        .await;
    assert_eq!(
        missing
            .expect_err("an unresolvable message errors")
            .to_string(),
        SummariseMessageError::Unresolved.to_string(),
        "the failure names the resolution, not the model",
    );
}

#[tokio::test]
async fn the_style_pull_infers_from_sent_mail_and_answers_none_without_it() {
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox(), fakes::sent()],
        vec![fakes::delivered_at(
            fakes::mail("s1", "sent", "Shipped"),
            7,
            0,
        )],
        vec![source("s1")],
    );
    let complete = fakes::ai::FixtureComplete::scripted(&[STYLE]);
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = drafted_app(provider, complete, Arc::clone(&store), None).await;

    let note = app.account_style_note(&message("m3").account).await;
    assert_eq!(
        note.as_deref(),
        Some("Crisp and warm, signs with thanks."),
        "the pull inferred the account's note from its Sent mail",
    );

    // No Sent mail at all: no profile is built and none is invented. A
    // fresh store, so the profile the first app saved cannot answer.
    let bare = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::delivered_at(
            fakes::mail("m1", "inbox", "Launch"),
            8,
            0,
        )],
        vec![source("m1")],
    );
    let complete = fakes::ai::FixtureComplete::scripted(&[STYLE]);
    let bare_store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = drafted_app(bare, complete, bare_store, None).await;
    assert_eq!(
        app.account_style_note(&message("m1").account).await,
        None,
        "no Sent mail, no profile",
    );
}

#[tokio::test]
async fn without_the_bundle_every_ai_pull_fails_quiet() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(Vec::new(), &surfaces);

    assert!(
        app.draft_chips(&message("m3").account, &message("m3").key)
            .await
            .is_empty(),
        "no bundle, no chips",
    );
    assert_eq!(
        app.summarise_message(&message("m3").account, &message("m3").key)
            .await
            .expect_err("the pull refuses")
            .to_string(),
        SummariseMessageError::NoBundle.to_string(),
    );
    assert_eq!(
        app.account_style_note(&message("m3").account).await,
        None,
        "no bundle, no note",
    );
    app.dismiss_chips(&message("m3").account, &message("m3").key);
}
