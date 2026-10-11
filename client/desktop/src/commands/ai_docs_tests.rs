// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the AI documents commands: the digest and report pulls
//! round trip through the fake app with their days clamped at the
//! command boundary (the default seven, the floor of one, the ceiling
//! of thirty-one), a generation's typed failure maps onto the command
//! error the webview sees with no content riding it, the latest read
//! crosses the kind as its wire word, the split pull round trips the
//! stored derivation's entries, and every view crosses as the wire
//! literals the web service pins on its side.

use std::sync::Arc;

use astra_ai::AiDocumentKind;
use astra_app::{DigestView, DocumentError, DocumentView, SplitEntry, SplitSide};

use super::{digest_generate_of, latest_document_of, report_generate_of, split_map_of};
use crate::commands::CommandError;
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The fixture account the AI pulls name.
fn account() -> engine_api::AccountId {
    engine_api::AccountId::try_from("work").expect("an account id")
}

/// One seeded generation answer, the shape both generate pulls answer.
fn generated(content: &str) -> DigestView {
    DigestView {
        period_start: 1,
        period_end: 2,
        content: content.to_owned(),
        regenerated: false,
    }
}

#[test]
fn the_digest_pull_round_trips_and_clamps_its_days_at_the_boundary() {
    let app = FakeApp::new();
    app.docs.lock().unwrap().generated = Ok(generated("Two replies wait."));
    let state = state("ai-docs-digest", Arc::clone(&app), FakeStore::accepting());

    let account = account();
    let view = state
        .runtime
        .block_on(|| digest_generate_of(&state, &account, None))
        .expect("the pull answers");
    assert_eq!(view, generated("Two replies wait."));
    assert_eq!(
        app.docs.lock().unwrap().generated_into.as_slice(),
        [("work".to_owned(), "digest".to_owned(), 7u32)].as_slice(),
        "an absent days crossed as the default seven",
    );

    let _ = state
        .runtime
        .block_on(|| digest_generate_of(&state, &account, Some(0)));
    let _ = state
        .runtime
        .block_on(|| digest_generate_of(&state, &account, Some(400)));
    assert_eq!(
        app.docs.lock().unwrap().generated_into.as_slice(),
        [
            ("work".to_owned(), "digest".to_owned(), 7u32),
            ("work".to_owned(), "digest".to_owned(), 1u32),
            ("work".to_owned(), "digest".to_owned(), 31u32),
        ]
        .as_slice(),
        "a named day count clamps to the window's floor and ceiling before \
         it crosses, so the core's collection and its prompt agree",
    );
}

#[test]
fn the_report_pull_round_trips_the_same_shape() {
    let app = FakeApp::new();
    app.docs.lock().unwrap().generated = Ok(generated("A launch week."));
    let state = state("ai-docs-report", Arc::clone(&app), FakeStore::accepting());

    let account = account();
    let view = state
        .runtime
        .block_on(|| report_generate_of(&state, &account, Some(3)))
        .expect("the pull answers");
    assert_eq!(view, generated("A launch week."));
    assert_eq!(
        app.docs.lock().unwrap().generated_into.as_slice(),
        [("work".to_owned(), "report".to_owned(), 3u32)].as_slice(),
        "the report pull crossed under its own kind word with its days",
    );
}

#[test]
fn a_generation_failure_maps_onto_the_command_error() {
    let app = FakeApp::new();
    app.docs.lock().unwrap().generated = Err(DocumentError::NoBundle);
    let state = state("ai-docs-refused", Arc::clone(&app), FakeStore::accepting());

    let account = account();
    let refused = state
        .runtime
        .block_on(|| digest_generate_of(&state, &account, None))
        .expect_err("the refusal crosses as the command error");
    assert_eq!(
        refused.to_string(),
        CommandError::Ai("no ai bundle is running".to_owned()).to_string(),
        "the typed failure maps onto the sentence the webview sees",
    );
}

#[test]
fn the_latest_pull_round_trips_the_kind_word_or_answers_none() {
    let app = FakeApp::new();
    app.docs.lock().unwrap().latest = Some(DocumentView {
        kind: AiDocumentKind::Digest,
        period_start: 1,
        period_end: 2,
        content: "Two replies wait.".to_owned(),
        created_at: 3,
    });
    let state = state("ai-docs-latest", Arc::clone(&app), FakeStore::accepting());

    let account = account();
    let latest = latest_document_of(&state, &account, AiDocumentKind::Digest)
        .expect("the seeded row answers");
    assert_eq!(latest.content, "Two replies wait.");
    assert!(
        latest_document_of(&state, &account, AiDocumentKind::Report).is_none(),
        "an unseeded kind answers null",
    );
    assert_eq!(
        app.docs.lock().unwrap().latest_into.as_slice(),
        [
            ("work".to_owned(), "digest".to_owned()),
            ("work".to_owned(), "report".to_owned()),
        ]
        .as_slice(),
        "the kind crossed as its snake_case wire word",
    );
}

#[test]
fn the_split_pull_round_trips_the_stored_derivation_s_entries() {
    let app = FakeApp::new();
    app.docs.lock().unwrap().split = vec![
        SplitEntry {
            key: "m1".to_owned(),
            side: SplitSide::Actioned,
        },
        SplitEntry {
            key: "m2".to_owned(),
            side: SplitSide::Fyi,
        },
    ];
    let state = state("ai-docs-split", Arc::clone(&app), FakeStore::accepting());

    let account = account();
    let entries = state.runtime.block_on(|| split_map_of(&state, &account));
    assert_eq!(
        entries,
        vec![
            SplitEntry {
                key: "m1".to_owned(),
                side: SplitSide::Actioned,
            },
            SplitEntry {
                key: "m2".to_owned(),
                side: SplitSide::Fyi,
            },
        ],
        "the pull answers the core's derivation whole",
    );
    assert_eq!(
        app.docs.lock().unwrap().split_into.as_slice(),
        ["work".to_owned()].as_slice(),
        "the command handed the webview's account to the core",
    );
}

#[test]
fn the_document_views_cross_as_their_wire_literals() {
    let generated = generated("All quiet now.");
    let wire = serde_json::to_value(&generated).expect("the view serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "period_start": 1,
            "period_end": 2,
            "content": "All quiet now.",
            "regenerated": false
        }),
    );

    let latest = DocumentView {
        kind: AiDocumentKind::Report,
        period_start: 1,
        period_end: 2,
        content: "A launch week.".to_owned(),
        created_at: 3,
    };
    let wire = serde_json::to_value(&latest).expect("the view serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "kind": "report",
            "period_start": 1,
            "period_end": 2,
            "content": "A launch week.",
            "created_at": 3
        }),
    );

    let entry = SplitEntry {
        key: "m1".to_owned(),
        side: SplitSide::Fyi,
    };
    let wire = serde_json::to_value(&entry).expect("the entry serialises");
    assert_eq!(wire, serde_json::json!({"key": "m1", "side": "fyi"}));
}
