// SPDX-License-Identifier: GPL-3.0-only
//! Knowledge service tests, over the fixture completion seam and an
//! in-memory knowledge store: the extraction path (the KnowledgeExtract
//! use case under the turn's correlation, the structured shape with each
//! entry's message attribution, the repair path, the insert loop's quiet
//! dedup skip), the failure paths (nothing lands, never a crash), the
//! pins (the thirty-message batch, each message in its own untrusted
//! wrap, the body cap, the version), and the context shaping (the
//! kind-ordered ten-entry cap, the recency order inside it, the
//! per-account read).

use astra_ai::{
    AiStore, CorrelationId, KnowledgeEntryRow, KnowledgeId, LlmUseCase, NewKnowledgeEntry,
};
use engine_api::{AccountId, ProviderKey};

use super::knowledge::{
    context_lines, extract_knowledge, extraction_prompt, knowledge_context, KnowledgeKind,
    KNOWLEDGE_BODY_CAP, KNOWLEDGE_CONTEXT_LIMIT, KNOWLEDGE_EXTRACT_LIMIT,
    KNOWLEDGE_EXTRACT_VERSION,
};
use crate::ai_rules::MailContext;
use crate::dispatch_fakes::ai::FixtureComplete;

/// The account whose knowledge the tests shape.
fn account() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// One mail message, `at` naming its position in the newest-first list
/// the caller collects.
fn message(at: usize) -> MailContext {
    MailContext {
        account: account(),
        key: ProviderKey::new(format!("m{at}")).unwrap(),
        from_name: "Ada Lovelace".to_owned(),
        from_address: "ada@example.test".to_owned(),
        subject: format!("Note {at}"),
        body: format!("Body {at}: the launch lands on Friday the {at}th."),
    }
}

/// The good fixture: two entries citing the batch's two messages (the
/// oldest renders as Message 1, the newest as Message 2).
const GOOD: &str = r#"{"entries":[
    {"kind":"preference","text":"The user prefers plain text replies.","message":2},
    {"kind":"fact","text":"The launch lands on Friday.","message":1}
]}"#;

/// One store row with every field set, the caller owning `created_at` so
/// the ordering tests control recency.
fn row(kind: &str, text: &str, created_at: i64) -> KnowledgeEntryRow {
    let length = text.len();
    KnowledgeEntryRow {
        id: KnowledgeId::new(format!("k{created_at}-{kind}-{length}")),
        account: "work".to_owned(),
        kind: kind.to_owned(),
        text: text.to_owned(),
        source_message_account: "work".to_owned(),
        source_message_key: "m-1".to_owned(),
        correlation: CorrelationId::new("c-1".to_owned()),
        created_at,
    }
}

#[tokio::test]
async fn a_fresh_extraction_lands_entries_with_their_sources() {
    let complete = FixtureComplete::scripted(&[GOOD]);
    let store = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    let landed = extract_knowledge(
        &[message(0), message(1)],
        &account(),
        &correlation,
        &complete,
        &store,
    )
    .await;
    assert_eq!(landed.len(), 2, "both entries land");
    assert_eq!(landed[0].kind, KnowledgeKind::Preference);
    assert_eq!(landed[0].text, "The user prefers plain text replies.");
    assert_eq!(
        landed[0].source_key.as_str(),
        "m0",
        "Message 2 is the newest message of the batch",
    );
    assert_eq!(landed[1].kind, KnowledgeKind::Fact);
    assert_eq!(
        landed[1].source_key.as_str(),
        "m1",
        "Message 1 is the oldest message of the batch",
    );
    assert_eq!(landed[0].source_account, account());
    assert_ne!(landed[0].id, landed[1].id, "each entry mints its id");
    assert!(landed[0].created_at > 0, "the store stamps the landing");
    assert_eq!(complete.call_count(), 1, "the batch is one call");
    let call = &complete.calls()[0];
    assert_eq!(call.use_case, LlmUseCase::KnowledgeExtract);
    assert_eq!(call.correlation, correlation);
    let stored = store.knowledge_for_account("work").unwrap();
    assert_eq!(stored.len(), 2);
    assert!(
        stored.iter().all(|entry| entry.correlation == correlation),
        "the landed rows carry the extraction turn",
    );
}

#[tokio::test]
async fn the_dedup_guard_skips_repeats_quietly() {
    let store = AiStore::open_in_memory().unwrap();
    store
        .insert_knowledge(&NewKnowledgeEntry {
            id: KnowledgeId::new("pre-stored".to_owned()),
            account: "work".to_owned(),
            kind: "fact".to_owned(),
            text: "The launch lands on Friday.".to_owned(),
            source_message_account: "work".to_owned(),
            source_message_key: "m9".to_owned(),
            correlation: CorrelationId::new("c-old".to_owned()),
        })
        .unwrap();
    let complete = FixtureComplete::scripted(&[r#"{"entries":[
        {"kind":"fact","text":"The launch lands on Friday.","message":1},
        {"kind":"preference","text":"The user prefers short replies.","message":1},
        {"kind":"preference","text":"The user prefers short replies.","message":2}
    ]}"#]);
    let landed = extract_knowledge(
        &[message(0), message(1)],
        &account(),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await;
    assert_eq!(
        landed.len(),
        1,
        "the stored text and the within-batch repeat both skip",
    );
    assert_eq!(landed[0].text, "The user prefers short replies.");
    let stored = store.knowledge_for_account("work").unwrap();
    assert_eq!(stored.len(), 2, "each text lands once and only once");
}

#[tokio::test]
async fn the_counted_run_names_the_entries_the_guard_skipped() {
    let store = AiStore::open_in_memory().unwrap();
    store
        .insert_knowledge(&NewKnowledgeEntry {
            id: KnowledgeId::new("pre-stored".to_owned()),
            account: "work".to_owned(),
            kind: "fact".to_owned(),
            text: "The launch lands on Friday.".to_owned(),
            source_message_account: "work".to_owned(),
            source_message_key: "m9".to_owned(),
            correlation: CorrelationId::new("c-old".to_owned()),
        })
        .unwrap();
    let complete = FixtureComplete::scripted(&[r#"{"entries":[
        {"kind":"fact","text":"The launch lands on Friday.","message":1},
        {"kind":"preference","text":"The user prefers short replies.","message":1}
    ]}"#]);
    let landing = super::knowledge::extract_knowledge_counted(
        &[message(0)],
        &account(),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await;
    assert_eq!(landing.landed.len(), 1, "the fresh text alone lands");
    assert_eq!(
        landing.skipped_duplicates, 1,
        "the held text is named as the one skip",
    );
}

#[tokio::test]
async fn an_unknown_kind_word_empties_the_batch() {
    let complete = FixtureComplete::scripted(&[r#"{"entries":[
        {"kind":"rumour","text":"The user hears things.","message":1}
    ]}"#]);
    let store = AiStore::open_in_memory().unwrap();
    let landed = extract_knowledge(
        &[message(0)],
        &account(),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await;
    assert!(landed.is_empty(), "the closed set rejects the word loudly");
    assert!(
        store.knowledge_for_account("work").unwrap().is_empty(),
        "nothing lands from a rejected batch",
    );
    assert_eq!(complete.call_count(), 1);
}

#[tokio::test]
async fn a_failed_call_lands_nothing_and_never_crashes() {
    let complete = FixtureComplete::failing("llm unreachable: all entries failed");
    let store = AiStore::open_in_memory().unwrap();
    let landed = extract_knowledge(
        &[message(0)],
        &account(),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await;
    assert!(landed.is_empty());
    assert!(store.knowledge_for_account("work").unwrap().is_empty());
}

#[tokio::test]
async fn garbage_output_lands_nothing() {
    let complete = FixtureComplete::scripted(&["no json here at all"]);
    let store = AiStore::open_in_memory().unwrap();
    let landed = extract_knowledge(
        &[message(0)],
        &account(),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await;
    assert!(landed.is_empty());
    assert!(store.knowledge_for_account("work").unwrap().is_empty());
}

#[tokio::test]
async fn prose_around_the_json_takes_the_repair_path() {
    let answer = format!("Sure, the knowledge:\n{GOOD}\nHope that helps!");
    let complete = FixtureComplete::scripted(&[&answer]);
    let store = AiStore::open_in_memory().unwrap();
    let landed = extract_knowledge(
        &[message(0), message(1)],
        &account(),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await;
    assert_eq!(landed.len(), 2, "the repaired completion lands its entries");
}

#[tokio::test]
async fn an_empty_batch_never_calls() {
    let complete = FixtureComplete::scripted(&[]);
    let store = AiStore::open_in_memory().unwrap();
    let landed = extract_knowledge(
        &[],
        &account(),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await;
    assert!(landed.is_empty());
    assert_eq!(complete.call_count(), 0, "no messages, no call");
}

#[tokio::test]
async fn an_out_of_range_citation_drops_only_its_entry() {
    let complete = FixtureComplete::scripted(&[r#"{"entries":[
        {"kind":"fact","text":"The launch lands on Friday.","message":1},
        {"kind":"fact","text":"The user never said this.","message":9}
    ]}"#]);
    let store = AiStore::open_in_memory().unwrap();
    let landed = extract_knowledge(
        &[message(0), message(1)],
        &account(),
        &CorrelationId::generate(),
        &complete,
        &store,
    )
    .await;
    assert_eq!(landed.len(), 1, "only the citable entry lands");
    assert_eq!(landed[0].text, "The launch lands on Friday.");
}

#[test]
fn the_batch_cap_keeps_the_newest_thirty() {
    let messages: Vec<MailContext> = (0..35).map(message).collect();
    let prompt = extraction_prompt(&messages);
    assert_eq!(KNOWLEDGE_EXTRACT_LIMIT, 30, "the cap is the spec's pin");
    assert_eq!(
        prompt.matches("<untrusted heading=\"email\">").count(),
        30,
        "the newest thirty messages ride, one wrap each",
    );
    assert!(prompt.contains("Body 0:"), "the newest message rides");
    assert!(prompt.contains("Body 29:"), "the thirtieth message rides");
    assert!(!prompt.contains("Body 30:"), "the thirty-first never rides");
    assert!(!prompt.contains("Body 34:"), "the oldest never rides");
}

#[test]
fn the_prompt_pins_its_halves_and_the_schema() {
    let prompt = extraction_prompt(&[message(0), message(1)]);
    assert!(
        prompt.matches("<untrusted heading=\"email\">").count() == 2,
        "each message rides its own untrusted wrap:\n{prompt}",
    );
    assert!(
        prompt.contains("Message 1:\n") && prompt.contains("Message 2:\n"),
        "the trusted numbering rides outside the wraps:\n{prompt}",
    );
    assert!(
        prompt.contains("From: Ada Lovelace"),
        "the rendering carries the sender and subject:\n{prompt}",
    );
    assert!(
        prompt.contains(r#"{"entries":[{"kind":"fact","text":"...","message":1}]}"#),
        "the schema example rides:\n{prompt}",
    );
    assert_eq!(KNOWLEDGE_EXTRACT_VERSION, "1");
}

#[test]
fn an_over_cap_body_carries_the_truncation_marker() {
    let mut long = message(0);
    long.body = format!("{}TAILMARK", "x".repeat(KNOWLEDGE_BODY_CAP + 50));
    let prompt = extraction_prompt(std::slice::from_ref(&long));
    assert!(
        prompt.contains("[truncated]"),
        "an over-cap body carries the truncation marker:\n{prompt}",
    );
    assert!(
        !prompt.contains("TAILMARK"),
        "the text past the cap never rides:\n{prompt}",
    );
}

#[test]
fn the_context_orders_kinds_first_then_recency() {
    let rows = vec![
        row("odd", "Foreign word", 99),
        row("preference", "Preference newest", 95),
        row("fact", "Fact newest", 90),
        row("contact", "Contact note", 80),
        row("project", "Project note", 60),
        row("persona", "Persona note", 50),
        row("preference", "Preference older", 40),
        row("fact", "Fact older", 30),
    ];
    let lines = context_lines(&rows);
    let words: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(
        words,
        vec![
            "persona: Persona note",
            "preference: Preference newest",
            "preference: Preference older",
            "odd: Foreign word",
            "fact: Fact newest",
            "contact: Contact note",
            "project: Project note",
            "fact: Fact older",
        ],
        "persona and preferences ride first, then the rest newest first",
    );
}

#[test]
fn the_context_caps_at_ten_entries() {
    let mut rows: Vec<KnowledgeEntryRow> = (0..12)
        .rev()
        .map(|at| row("fact", &format!("Fact {at:02}"), at))
        .collect();
    rows.push(row("preference", "Preference note", 98));
    rows.push(row("persona", "Persona note", 99));
    let lines = context_lines(&rows);
    assert_eq!(KNOWLEDGE_CONTEXT_LIMIT, 10, "the cap is the spec's pin");
    assert_eq!(lines.len(), 10);
    assert!(lines.contains(&"persona: Persona note".to_owned()));
    assert!(lines.contains(&"preference: Preference note".to_owned()));
    assert!(lines.contains(&"fact: Fact 11".to_owned()));
    assert!(lines.contains(&"fact: Fact 04".to_owned()));
    assert!(
        !lines.contains(&"fact: Fact 03".to_owned()),
        "the oldest facts past the cap never ride",
    );
}

#[test]
fn the_context_read_stays_per_account_and_survives_its_store() {
    let store = AiStore::open_in_memory().unwrap();
    store
        .insert_knowledge(&NewKnowledgeEntry {
            id: KnowledgeId::new("k-1".to_owned()),
            account: "work".to_owned(),
            kind: "preference".to_owned(),
            text: "The user prefers plain text mail.".to_owned(),
            source_message_account: "work".to_owned(),
            source_message_key: "m-1".to_owned(),
            correlation: CorrelationId::new("c-1".to_owned()),
        })
        .unwrap();
    assert!(
        knowledge_context(&AccountId::try_from("home").unwrap(), &store).is_empty(),
        "another account's knowledge never rides",
    );
    let lines = knowledge_context(&account(), &store);
    assert_eq!(
        lines,
        vec!["preference: The user prefers plain text mail.".to_owned()],
    );
    assert!(
        knowledge_context(&AccountId::try_from("empty").unwrap(), &store).is_empty(),
        "an untouched account answers nothing",
    );
}
