// SPDX-License-Identifier: GPL-3.0-only
//! The AI documents driver's split and quiet-answer tests, split from
//! the generation pulls so each file stays under the 500-line limit:
//! the split map serves the stored categorisation over the newest
//! window with no model call spent, an empty window makes no call and
//! stores its honest nothing, an app with no AI bundle refuses the
//! generations and answers the reads quiet, and the views cross as the
//! wire fields the desktop command tests mirror.

use super::*;

/// The split map serves the stored categorisation over the newest
/// window: every mapped word its pinned side in key order, a word the
/// table excludes simply absent, an uncategorised message never
/// entering, and no model call spent at all.
#[tokio::test]
async fn the_split_map_serves_the_stored_rows_with_no_call_spent() {
    let mut messages = Vec::new();
    let mut sources = Vec::new();
    for at in 1..=4 {
        let key = format!("m{at}");
        messages.push(inbox_mail(&key, &format!("Note {at}"), 8 + at));
        sources.push(source(&key));
    }
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    categorised(&store, "m1", "primary");
    categorised(&store, "m2", "newsletter");
    categorised(&store, "m3", "rumour");
    let complete = fakes::ai::FixtureComplete::answering(DIGESTED);
    let probe = complete.clone();
    let app = docs_app(
        fakes::FakeProvider::with_sources(vec![fakes::inbox()], messages, sources),
        complete,
        Arc::clone(&store),
    )
    .await;

    let entries = app.split_map(&account()).await;
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
        "the mapped words serve their sides in key order, the foreign word \
         and the uncategorised message absent",
    );
    assert_eq!(
        probe.call_count(),
        0,
        "the split map spends no model call at all",
    );
}

/// An empty window makes no call, answers the empty content, and still
/// stores its period's row: the panel renders an honest nothing.
#[tokio::test]
async fn an_empty_window_generates_no_call_and_stores_the_empty_document() {
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let complete = fakes::ai::FixtureComplete::answering(DIGESTED);
    let probe = complete.clone();
    // Mail delivered on the first of the month: outside a window over
    // the fixture day entirely.
    let app = docs_app(
        fakes::FakeProvider::with_sources(
            vec![fakes::inbox()],
            vec![on_the_first(fakes::mail("old", "inbox", "Ancient note"))],
            vec![source("old")],
        ),
        complete,
        Arc::clone(&store),
    )
    .await;

    let view = app
        .generate_document(&account(), 1, AiDocumentKind::Digest, day_end())
        .await
        .expect("the empty window answers, not fails");
    assert_eq!(
        view.content, "",
        "nothing arrived, so nothing is summarised"
    );
    assert!(!view.regenerated);
    assert_eq!(probe.call_count(), 0, "the empty window spent no call");
    assert!(
        app.latest_document(&account(), AiDocumentKind::Digest)
            .is_some(),
        "the empty document is stored under its period",
    );
}

/// Without the AI bundle the generations refuse honestly and the reads
/// answer quiet.
#[tokio::test]
async fn without_the_bundle_the_generations_refuse_and_the_reads_answer_quiet() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(Vec::new(), &surfaces);

    for kind in [AiDocumentKind::Digest, AiDocumentKind::Report] {
        let refused = app
            .generate_document(&account(), 7, kind, day_end())
            .await
            .expect_err("no bundle, no generation");
        assert_eq!(
            refused.to_string(),
            DocumentError::NoBundle.to_string(),
            "the refusal is the typed no-bundle failure",
        );
    }
    assert!(
        app.latest_document(&account(), AiDocumentKind::Digest)
            .is_none(),
        "no bundle, no store to read",
    );
    assert!(
        app.split_map(&account()).await.is_empty(),
        "no bundle, no split to serve",
    );
}

/// The generation view's wire shape is the four fields the host command
/// answers: the pin the desktop command tests mirror on their side of
/// the wire.
#[test]
fn the_generation_and_latest_views_serialise_as_their_wire_fields() {
    let generated = DigestView {
        period_start: 1,
        period_end: 2,
        content: "All quiet now.".to_owned(),
        regenerated: true,
    };
    let wire = serde_json::to_value(&generated).expect("the view serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "period_start": 1,
            "period_end": 2,
            "content": "All quiet now.",
            "regenerated": true
        }),
    );

    let entry = SplitEntry {
        key: "m1".to_owned(),
        side: SplitSide::Actioned,
    };
    let wire = serde_json::to_value(&entry).expect("the entry serialises");
    assert_eq!(wire, serde_json::json!({"key": "m1", "side": "actioned"}));

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
}
