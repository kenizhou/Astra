// SPDX-License-Identifier: GPL-3.0-only
//! `drafts save` and `drafts discard` tests: the round trip through a
//! provider that stores (the fixture), the parked-op answer through the
//! offline provider, and the preview's equality with what a run actually
//! puts.

use engine_api::{AccountId, Engine};

use super::drafts;
use super::fakes::{inbox, seed, FakeProvider};
use super::{DraftArgs, Offline};

fn account() -> AccountId {
    AccountId::try_from("alpha").unwrap()
}

fn args() -> DraftArgs {
    DraftArgs {
        account: "alpha".to_owned(),
        from: "me@example.test".to_owned(),
        to: vec![
            "ada@example.test".to_owned(),
            "grace@example.test".to_owned(),
        ],
        cc: vec!["alan@example.test".to_owned()],
        subject: "Quarterly report".to_owned(),
        body: "the words of the draft".to_owned(),
        source: "arg",
    }
}

#[tokio::test]
async fn drafts_save_stores_the_built_draft_and_answers_stored() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(vec![inbox()], Vec::new());
    seed(&engine, &provider, &account()).await;

    let answer = drafts::save(&engine, &provider, &account(), &args(), false)
        .await
        .unwrap();
    assert_eq!(answer["state"], "stored");
    assert!(answer["message_id"]
        .as_str()
        .unwrap()
        .ends_with("@astra.local"));
    let puts = provider.draft_puts();
    assert_eq!(puts.len(), 1, "one engine put per save");
    assert_eq!(puts[0].0.subject, "Quarterly report");
    assert_eq!(puts[0].0.text_body, "the words of the draft");
    assert_eq!(puts[0].1, None, "a first save supersedes nothing");
    let recorded = &puts[0].0;
    assert_eq!(recorded.from.email, "me@example.test");
    assert_eq!(recorded.to.len(), 2);
    assert_eq!(recorded.cc.len(), 1);
}

#[tokio::test]
async fn drafts_save_parks_the_op_when_the_provider_leg_fails() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(vec![inbox()], Vec::new());
    seed(&engine, &provider, &account()).await;

    let answer = drafts::save(&engine, &Offline, &account(), &args(), false)
        .await
        .unwrap();
    assert_eq!(answer["state"], "queued", "the durable op is the answer");
    assert!(answer["op"].as_u64().unwrap() > 0);
    assert!(
        answer["detail"]
            .as_str()
            .unwrap()
            .contains("dials no server"),
        "the detail names the offline contract: {}",
        answer["detail"]
    );
    // The parked op is real: the outbox holds it for the app's drain.
    let rows = engine.outbox(&account()).await.unwrap();
    assert!(rows
        .iter()
        .any(|row| row.resource_key.as_str().starts_with("draft:")));
}

#[tokio::test]
async fn the_preview_prints_the_same_draft_a_run_puts() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(vec![inbox()], Vec::new());
    seed(&engine, &provider, &account()).await;

    let preview = drafts::save(&engine, &provider, &account(), &args(), true)
        .await
        .unwrap();
    assert_eq!(preview["action"], "put_draft");
    assert_eq!(preview["account"], "alpha");
    let shown = &preview["draft"];
    assert_eq!(shown["from"], "me@example.test");
    assert_eq!(
        shown["to"],
        serde_json::json!(["ada@example.test", "grace@example.test"])
    );
    assert_eq!(shown["cc"], serde_json::json!(["alan@example.test"]));
    assert_eq!(shown["subject"], "Quarterly report");
    assert_eq!(shown["body_chars"], 22);
    assert!(
        !shown.to_string().contains("the words of the draft"),
        "the preview shows the body's length, never its text"
    );

    // Equality against what the engine call actually carried: the same
    // builder feeds both, so the fields must agree one for one.
    drafts::save(&engine, &provider, &account(), &args(), false)
        .await
        .unwrap();
    let (put, _) = provider.draft_puts()[0].clone();
    assert_eq!(put.subject, shown["subject"].as_str().unwrap());
    assert_eq!(put.from.email, shown["from"].as_str().unwrap());
    assert_eq!(
        put.to.iter().map(|a| a.email.clone()).collect::<Vec<_>>(),
        shown["to"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        u64::try_from(put.text_body.chars().count()).unwrap(),
        shown["body_chars"].as_u64().unwrap()
    );
}

#[tokio::test]
async fn drafts_discard_removes_the_key_and_answers_removed() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(vec![inbox()], Vec::new());
    seed(&engine, &provider, &account()).await;

    let answer = drafts::discard(&engine, &provider, &account(), "draft-key-1", false)
        .await
        .unwrap();
    assert_eq!(answer["state"], "removed");
    assert_eq!(answer["key"], "draft-key-1");
    assert_eq!(
        provider.draft_deletes(),
        vec![engine_api::ProviderKey::new("draft-key-1").unwrap()]
    );
}

#[tokio::test]
async fn drafts_discard_parks_the_op_when_the_provider_leg_fails() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(vec![inbox()], Vec::new());
    seed(&engine, &provider, &account()).await;

    let answer = drafts::discard(&engine, &Offline, &account(), "draft-key-1", false)
        .await
        .unwrap();
    assert_eq!(answer["state"], "queued");
    assert!(answer["op"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn the_discard_preview_names_the_exact_delete() {
    let engine = Engine::open_in_memory().unwrap();
    let preview = drafts::discard(&engine, &Offline, &account(), "draft-key-1", true)
        .await
        .unwrap();
    assert_eq!(preview["action"], "delete_draft");
    assert_eq!(preview["account"], "alpha");
    assert_eq!(preview["key"], "draft-key-1");
}

#[tokio::test]
async fn a_malformed_discard_key_is_a_usage_error() {
    let engine = Engine::open_in_memory().unwrap();
    let answer = drafts::discard(&engine, &Offline, &account(), "", false).await;
    assert_eq!(answer.unwrap_err().0.exit(), 2);
}
