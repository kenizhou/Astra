// SPDX-License-Identifier: GPL-3.0-only
//! `messages list` and `messages read` tests, over an in-memory engine
//! seeded the way a first sync would fill it.

use engine_api::{AccountId, Engine};

use super::fakes::{folder, inbox, mail, read, seed, FakeProvider};
use super::messages;
use super::Offline;

fn account() -> AccountId {
    AccountId::try_from("alpha").unwrap()
}

#[tokio::test]
async fn messages_list_answers_newest_first_within_the_limit() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(
        vec![inbox()],
        vec![
            mail("m1", "inbox", "oldest", 8),
            mail("m2", "inbox", "middle", 9),
            mail("m3", "inbox", "newest", 10),
        ],
    );
    seed(&engine, &provider, &account()).await;

    let answer = messages::list(&engine, &account(), None, 2).await.unwrap();
    let keys: Vec<&str> = answer["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["m3", "m2"]);
}

#[tokio::test]
async fn one_row_carries_the_envelope_facts_an_agent_picks_with() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(
        vec![inbox()],
        vec![read(mail("m1", "inbox", "Quarterly report", 8))],
    );
    seed(&engine, &provider, &account()).await;

    let answer = messages::list(&engine, &account(), None, 10).await.unwrap();
    let row = &answer["messages"][0];
    assert_eq!(row["key"], "m1");
    assert_eq!(row["from"], "ada@example.test");
    assert_eq!(row["from_name"], "Ada Lovelace");
    assert_eq!(row["subject"], "Quarterly report");
    assert_eq!(row["date"], "2026-10-09T08:00:00Z");
    assert_eq!(row["unread"], false);
    assert_eq!(row["draft"], false);
}

#[tokio::test]
async fn a_folder_filter_slices_that_folders_page() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(
        vec![inbox(), folder("archive", "Archive")],
        vec![
            mail("m1", "inbox", "in the inbox", 10),
            mail("m2", "archive", "in the archive", 9),
            mail("m3", "archive", "also archived", 8),
        ],
    );
    seed(&engine, &provider, &account()).await;

    let answer = messages::list(&engine, &account(), Some("archive"), 10)
        .await
        .unwrap();
    assert_eq!(answer["folder"], "archive");
    let keys: Vec<&str> = answer["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["m2", "m3"]);
}

#[tokio::test]
async fn messages_read_answers_the_whole_message_with_its_cached_body() {
    let engine = Engine::open_in_memory().unwrap();
    let raw = "From: ada@example.test\r\nTo: me@example.test\r\n\
               Subject: Quarterly report\r\n\r\nthe reading view";
    let provider = FakeProvider::with_sources(
        vec![inbox()],
        vec![mail("m1", "inbox", "Quarterly report", 8)],
        vec![("m1", raw)],
    );
    seed(&engine, &provider, &account()).await;

    let answer = messages::read(
        &engine,
        &provider,
        &account(),
        &["m1".to_owned(), "absent".to_owned()],
    )
    .await
    .unwrap();
    assert_eq!(answer["asked"], 2);
    let list = answer["messages"].as_array().unwrap();
    assert_eq!(list.len(), 1, "an absent key is simply not answered");
    let row = &list[0];
    assert_eq!(row["key"], "m1");
    assert_eq!(row["subject"], "Quarterly report");
    assert_eq!(
        row["from"],
        serde_json::json!(["Ada Lovelace <ada@example.test>"])
    );
    assert_eq!(row["body"]["plain"], "the reading view");
    assert_eq!(row["unread"], true);
}

#[tokio::test]
async fn an_uncached_body_fails_the_read_under_the_offline_provider() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(vec![inbox()], vec![mail("m1", "inbox", "s", 8)]);
    seed(&engine, &provider, &account()).await;

    let answer = messages::read(&engine, &Offline, &account(), &["m1".to_owned()]).await;
    let (code, message) = answer.unwrap_err();
    assert_eq!(code.exit(), 1);
    assert!(
        message.contains("a body could not be read"),
        "the failure names the body read: {message}"
    );
}

#[tokio::test]
async fn a_malformed_key_is_a_usage_error() {
    let engine = Engine::open_in_memory().unwrap();
    let answer = messages::read(&engine, &Offline, &account(), &[String::new()]).await;
    assert_eq!(answer.unwrap_err().0.exit(), 2);
}
