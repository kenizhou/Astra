// SPDX-License-Identifier: GPL-3.0-only
//! AI assist driver tests: the extraction pull collects the account's
//! newest mail (oldest first in the batch, its citations resolving), a
//! smaller limit keeps only the newest, and a repeated run names the
//! texts the dedup guard skipped; the listing answers the account's rows
//! with foreign kind words passing through and the forget removes one
//! entry; the snippet listing and the ranked search (with its substring
//! fallback) round trip the store; the autocomplete pull answers the
//! trimmed continuation under its use case; the translate pull serves
//! the cache on the second read and answers empty where nothing
//! resolves; and an app with no AI bundle answers every pull quiet.

use std::sync::{Arc, Mutex};

use astra_ai::{
    AiStore, CorrelationId, JudgeOutcome, JudgePath, JudgeVerdict, KnowledgeId, NewKnowledgeEntry,
    NewSnippet, SnippetId,
};

use super::drive::{ExtractionSummary, KnowledgeView};
use super::knowledge::KNOWLEDGE_EXTRACT_LIMIT;
use super::translate::TranslateLanguage;
use crate::dispatch_fakes as fakes;
use crate::dispatch_fakes::ai::app_with_ai;
use crate::reference::MessageRef;
use crate::{App, Intent};

/// The extraction fixture answer: two entries citing the batch's middle
/// and newest messages (the oldest renders as Message 1).
const LANDED: &str = r#"{"entries":[
    {"kind":"fact","text":"The launch lands on Friday.","message":2},
    {"kind":"preference","text":"The user prefers plain text replies.","message":3}
]}"#;

/// The extraction answer a two-message batch cites: its Message 1 is the
/// second newest message of the three the account holds.
const NEWEST_ONLY: &str = r#"{"entries":[
    {"kind":"fact","text":"The launch lands on Friday.","message":1}
]}"#;

/// A first-try verdict outcome, the bundle's unused judge demand.
fn yes() -> JudgeOutcome {
    JudgeOutcome {
        verdict: JudgeVerdict::Yes,
        path: JudgePath::FirstTry,
    }
}

/// The fixture account every pull names.
fn account() -> engine_api::AccountId {
    engine_api::AccountId::try_from("work").unwrap()
}

/// A message reference for the fixture account.
fn message(key: &str) -> MessageRef {
    MessageRef {
        account: account(),
        key: engine_api::ProviderKey::new(key).unwrap(),
    }
}

/// A plain source for `key`, so the body reads have bytes to decode.
fn source(key: &str) -> (String, Vec<u8>) {
    (
        key.to_owned(),
        format!("From: ada@example.test\r\nSubject: Re: Note\r\n\r\nBody of {key}.\r\n")
            .into_bytes(),
    )
}

/// Three inbox messages for the fixture account: `m1` the oldest, `m2`,
/// and `m3` the newest, each with a plain source so bodies resolve.
fn inbox() -> fakes::FakeProvider {
    let mut messages = Vec::new();
    let mut sources = Vec::new();
    for (at, hour) in [(1, 8), (2, 9), (3, 10)] {
        let key = format!("m{at}");
        messages.push(fakes::delivered_at(
            fakes::mail(&key, "inbox", &format!("Note {at}")),
            hour,
            0,
        ));
        sources.push(source(&key));
    }
    fakes::FakeProvider::with_sources(vec![fakes::inbox()], messages, sources)
}

/// An app over `provider` with the AI bundle wired over `store`, synced
/// once so the engine holds the mail. The judge answers Yes; the assist
/// pulls never ask it, but the bundle demands one.
async fn assist_app(
    provider: fakes::FakeProvider,
    complete: fakes::ai::FixtureComplete,
    store: Arc<AiStore>,
) -> App<fakes::FakeProvider> {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_ai(
        vec![fakes::account("work", provider)],
        &surfaces,
        fakes::ai::FixtureJudge::answering(yes()),
        complete,
        store,
        astra_ai::DlpPolicy::Allow,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// Seeds one knowledge entry for the account, `id` and `kind` the
/// caller's, so the listing and the dedup guard have held rows.
fn seed_knowledge(store: &AiStore, id: &str, kind: &str, text: &str) {
    store
        .insert_knowledge(&NewKnowledgeEntry {
            id: KnowledgeId::new(id.to_owned()),
            account: "work".to_owned(),
            kind: kind.to_owned(),
            text: text.to_owned(),
            source_message_account: "work".to_owned(),
            source_message_key: "m9".to_owned(),
            correlation: CorrelationId::new("c-old".to_owned()),
        })
        .expect("the seed entry lands");
}

/// Seeds one snippet, `id` the caller's.
fn seed_snippet(store: &AiStore, id: &str, title: &str, body: &str) {
    store
        .insert_snippet(&NewSnippet {
            id: SnippetId::new(id.to_owned()),
            title: title.to_owned(),
            body: body.to_owned(),
        })
        .expect("the seed snippet lands");
}

#[tokio::test]
async fn the_extraction_pull_collects_the_newest_mail_and_lands_its_entries() {
    let complete = fakes::ai::FixtureComplete::answering(LANDED);
    let probe = complete.clone();
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = assist_app(inbox(), complete, Arc::clone(&store)).await;

    let summary = app
        .knowledge_extract(&account(), KNOWLEDGE_EXTRACT_LIMIT)
        .await;
    assert_eq!(
        summary,
        ExtractionSummary {
            extracted: 2,
            skipped_duplicates: 0,
            messages_considered: 3,
        },
        "the pull names what landed and what the batch carried",
    );

    let mut cited: Vec<String> = store
        .knowledge_for_account("work")
        .unwrap()
        .into_iter()
        .map(|row| row.source_message_key)
        .collect();
    cited.sort();
    assert_eq!(
        cited,
        vec!["m2".to_owned(), "m3".to_owned()],
        "each entry kept the key of the message its citation resolved to",
    );

    assert_eq!(probe.call_count(), 1, "the batch is one call");
    let call = &probe.calls()[0];
    assert_eq!(call.use_case, astra_ai::LlmUseCase::KnowledgeExtract);
    assert!(
        !call.correlation.as_str().is_empty(),
        "the pull minted the extraction turn's correlation",
    );
    let first = call.prompt.find("Note 1").expect("the oldest renders");
    let second = call.prompt.find("Note 2").expect("the middle renders");
    let third = call.prompt.find("Note 3").expect("the newest renders");
    assert!(
        first < second && second < third,
        "the batch renders oldest first, the order the citations number by",
    );
}

#[tokio::test]
async fn a_smaller_limit_keeps_only_the_newest_messages() {
    let complete = fakes::ai::FixtureComplete::scripted(&[NEWEST_ONLY]);
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = assist_app(inbox(), complete, Arc::clone(&store)).await;

    let summary = app.knowledge_extract(&account(), 2).await;
    assert_eq!(
        summary,
        ExtractionSummary {
            extracted: 1,
            skipped_duplicates: 0,
            messages_considered: 2,
        },
        "the window kept the two newest messages only",
    );
    let cited: Vec<String> = store
        .knowledge_for_account("work")
        .unwrap()
        .into_iter()
        .map(|row| row.source_message_key)
        .collect();
    assert_eq!(
        cited,
        vec!["m2".to_owned()],
        "the batch's Message 1 is the second newest message of the three",
    );
}

#[tokio::test]
async fn a_repeated_extraction_names_the_texts_the_guard_skipped() {
    let complete = fakes::ai::FixtureComplete::answering(LANDED);
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = assist_app(inbox(), complete, store).await;

    let first = app
        .knowledge_extract(&account(), KNOWLEDGE_EXTRACT_LIMIT)
        .await;
    assert_eq!(first.extracted, 2, "the first run lands both entries");
    let second = app
        .knowledge_extract(&account(), KNOWLEDGE_EXTRACT_LIMIT)
        .await;
    assert_eq!(
        second,
        ExtractionSummary {
            extracted: 0,
            skipped_duplicates: 2,
            messages_considered: 3,
        },
        "the second run lands nothing and names both dedup skips",
    );
}

#[tokio::test]
async fn the_listing_answers_the_rows_and_the_forget_removes_one() {
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    seed_knowledge(&store, "k1", "fact", "The launch lands on Friday.");
    seed_knowledge(&store, "k2", "habitude", "The user files mail at noon.");
    seed_knowledge(&store, "k3", "preference", "The user prefers plain text.");
    let app = assist_app(
        inbox(),
        fakes::ai::FixtureComplete::answering(LANDED),
        store,
    )
    .await;

    let listed = app.knowledge_list(&account());
    let ids: Vec<&str> = listed.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids, vec!["k3", "k2", "k1"], "the newest rows lead");
    assert!(
        listed.iter().any(|entry| entry.kind == "habitude"),
        "a foreign kind word passes through as it stands, never dropped or guessed",
    );
    assert_eq!(
        listed[0].text, "The user prefers plain text.",
        "the view carries the row's own text",
    );

    assert!(
        app.knowledge_forget(&KnowledgeId::new("k1".to_owned())),
        "the forget answers the row it removed",
    );
    assert!(
        !app.knowledge_forget(&KnowledgeId::new("k1".to_owned())),
        "a second forget of the same id answers false",
    );
    let remaining: Vec<String> = app
        .knowledge_list(&account())
        .into_iter()
        .map(|entry| entry.id.as_str().to_owned())
        .collect();
    assert_eq!(remaining, vec!["k3", "k2"], "only the forgotten row went");
}

#[tokio::test]
async fn the_snippet_listing_answers_the_library_newest_touched_first() {
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    seed_snippet(&store, "s1", "Greeting", "Thank you for the note.");
    seed_snippet(
        &store,
        "s2",
        "Invoice chase",
        "Attached is the invoice again.",
    );
    let app = assist_app(
        inbox(),
        fakes::ai::FixtureComplete::answering(LANDED),
        store,
    )
    .await;

    let listed = app.snippet_list();
    let ids: Vec<&str> = listed.iter().map(|snippet| snippet.id.as_str()).collect();
    assert_eq!(ids, vec!["s2", "s1"], "the most recently touched leads");
    assert_eq!(listed[0].title, "Invoice chase");
}

#[tokio::test]
async fn the_snippet_search_ranks_and_falls_back_when_the_call_fails() {
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    seed_snippet(&store, "s1", "Greeting", "Thank you for the note.");
    seed_snippet(
        &store,
        "s2",
        "Invoice chase",
        "Attached is the invoice again.",
    );

    let complete = fakes::ai::FixtureComplete::scripted(&[r#"{"matches":[2,1]}"#]);
    let probe = complete.clone();
    let app = assist_app(inbox(), complete, Arc::clone(&store)).await;
    let ranked = app.snippet_search("the money one").await;
    assert_eq!(
        ranked
            .iter()
            .map(|snippet| snippet.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Greeting", "Invoice chase"],
        "the ranking's positions resolved in their own order over the listed library",
    );
    let call = &probe.calls()[0];
    assert_eq!(call.use_case, astra_ai::LlmUseCase::SnippetSearch);
    assert!(
        !call.correlation.as_str().is_empty(),
        "the search minted its own turn",
    );

    let failing = fakes::ai::FixtureComplete::failing("the endpoint is down");
    let app = assist_app(inbox(), failing, store).await;
    let fell = app.snippet_search("INVOICE").await;
    assert_eq!(
        fell.iter()
            .map(|snippet| snippet.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Invoice chase"],
        "a failed call falls back to the substring search, case aside",
    );
}

#[tokio::test]
async fn the_autocomplete_pull_answers_the_trimmed_continuation() {
    let complete = fakes::ai::FixtureComplete::answering("  shipped it.  ");
    let probe = complete.clone();
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = assist_app(inbox(), complete, store).await;

    let suggestion = app.compose_autocomplete("Thanks for the", " soon.").await;
    assert_eq!(suggestion, "shipped it.", "the answer rides trimmed");
    let call = &probe.calls()[0];
    assert_eq!(call.use_case, astra_ai::LlmUseCase::Autocomplete);
    assert!(call.prompt.contains("Thanks for the"));
    assert!(call.prompt.contains("soon."), "the trailing half rode too");
    assert!(
        !call.correlation.as_str().is_empty(),
        "the ask minted its own turn",
    );
}

#[tokio::test]
async fn the_translate_pull_serves_the_cache_and_answers_empty_where_nothing_resolves() {
    let complete = fakes::ai::FixtureComplete::scripted(&[r#"{"translation":"Corps de m3."}"#]);
    let probe = complete.clone();
    let store = Arc::new(AiStore::open_in_memory().unwrap());
    let app = assist_app(inbox(), complete, store).await;

    let first = app
        .translate_message(&account(), &message("m3").key, TranslateLanguage::French)
        .await;
    assert_eq!(first, "Corps de m3.");
    let second = app
        .translate_message(&account(), &message("m3").key, TranslateLanguage::French)
        .await;
    assert_eq!(second, "Corps de m3.", "the second read answers the same");
    assert_eq!(
        probe.call_count(),
        1,
        "the cached second read spent no second call",
    );

    let missing = app
        .translate_message(&account(), &message("nope").key, TranslateLanguage::German)
        .await;
    assert_eq!(missing, "", "an unresolvable message answers empty");
    assert_eq!(probe.call_count(), 1, "the missing message spent no call");
}

#[tokio::test]
async fn without_the_bundle_every_assist_pull_answers_quiet() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(Vec::new(), &surfaces);

    assert!(app.knowledge_list(&account()).is_empty());
    assert_eq!(
        app.knowledge_extract(&account(), KNOWLEDGE_EXTRACT_LIMIT)
            .await,
        ExtractionSummary::default(),
        "no bundle, no run",
    );
    assert!(
        !app.knowledge_forget(&KnowledgeId::new("k1".to_owned())),
        "no bundle, nowhere to delete",
    );
    assert!(app.snippet_list().is_empty());
    assert!(app.snippet_search("anything").await.is_empty());
    assert_eq!(
        app.compose_autocomplete("Thanks", "").await,
        "",
        "no bundle, no suggestion",
    );
    assert_eq!(
        app.translate_message(&account(), &message("m3").key, TranslateLanguage::English)
            .await,
        "",
        "no bundle, no translation",
    );
}

/// The knowledge view's wire shape is the six fields the host lists: the
/// pin the desktop command tests mirror on their side of the wire.
#[test]
fn the_knowledge_view_serialises_as_its_six_wire_fields() {
    let view = KnowledgeView::from(astra_ai::KnowledgeEntryRow {
        id: KnowledgeId::new("k1".to_owned()),
        account: "work".to_owned(),
        kind: "preference".to_owned(),
        text: "The user prefers plain text.".to_owned(),
        source_message_account: "work".to_owned(),
        source_message_key: "m1".to_owned(),
        correlation: CorrelationId::new("c-1".to_owned()),
        created_at: 1,
    });
    let wire = serde_json::to_value(&view).expect("the view serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "id": "k1",
            "kind": "preference",
            "text": "The user prefers plain text.",
            "source_account": "work",
            "source_key": "m1",
            "created_at": 1
        }),
    );
}
