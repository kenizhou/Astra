// SPDX-License-Identifier: GPL-3.0-only
//! `messages list` and `messages read` tests, over an in-memory engine
//! seeded the way a first sync would fill it.

use engine_api::{AccountId, Engine};

use super::fakes::{flagged, folder, inbox, mail, read, role_folder, seed, FakeProvider};
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

fn beta() -> AccountId {
    AccountId::try_from("beta").unwrap()
}

/// Two seeded accounts: alpha holds a trash folder with a hit inside it, so
/// the scope rules have something to exclude.
async fn seeded_accounts() -> Engine {
    let engine = Engine::open_in_memory().unwrap();
    let alpha = FakeProvider::with(
        vec![
            inbox(),
            role_folder("bin", "Trash", engine_api::MailboxRole::Trash),
        ],
        vec![
            mail("a-new", "inbox", "Quarterly report", 10),
            mail("a-trashed", "bin", "Quarterly report trashed", 9),
        ],
    );
    seed(&engine, &alpha, &account()).await;
    let beta_provider = FakeProvider::with(
        vec![inbox()],
        vec![mail("b-mid", "inbox", "Re: Quarterly report", 8)],
    );
    seed(&engine, &beta_provider, &beta()).await;
    engine
}

fn row_keys(answer: &serde_json::Value) -> Vec<&str> {
    answer["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["key"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn search_without_a_named_account_covers_every_account_except_trash() {
    let engine = seeded_accounts().await;

    let answer = messages::search(&engine, &[account(), beta()], false, None, "report", 0, 20)
        .await
        .unwrap();
    // The trashed hit (hour 9) would sort between the two without the
    // exclusion, so the order pins both the scope and the newest-first cut.
    assert_eq!(row_keys(&answer), vec!["a-new", "b-mid"]);
    assert_eq!(answer["total"], 2);
    assert_eq!(answer["offset"], 0);
    assert_eq!(answer["has_more"], false);
    assert_eq!(answer["windowed"], false);
}

#[tokio::test]
async fn one_search_row_carries_the_facts_an_agent_acts_on() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(
        vec![inbox()],
        vec![flagged(mail("m1", "inbox", "Quarterly report", 8))],
    );
    seed(&engine, &provider, &account()).await;

    let answer = messages::search(&engine, &[account()], false, None, "report", 0, 20)
        .await
        .unwrap();
    let row = &answer["rows"][0];
    assert_eq!(row["account"], "alpha");
    assert_eq!(row["key"], "m1");
    assert_eq!(row["subject"], "Quarterly report");
    assert_eq!(row["sender"], "Ada Lovelace <ada@example.test>");
    assert_eq!(row["date"], "2026-10-09T08:00:00Z");
    assert_eq!(row["unread"], true);
    assert_eq!(row["flagged"], true);
    assert_eq!(row["has_attachment"], false);
}

#[tokio::test]
async fn a_named_account_searches_its_whole_mailbox_including_trash() {
    let engine = seeded_accounts().await;

    let answer = messages::search(&engine, &[account()], true, None, "report", 0, 20)
        .await
        .unwrap();
    assert_eq!(row_keys(&answer), vec!["a-new", "a-trashed"]);
}

#[tokio::test]
async fn a_folder_narrows_the_named_account() {
    let engine = seeded_accounts().await;

    let answer = messages::search(&engine, &[account()], true, Some("bin"), "report", 0, 20)
        .await
        .unwrap();
    assert_eq!(row_keys(&answer), vec!["a-trashed"]);
}

#[tokio::test]
async fn search_pages_with_a_has_more_answer() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(
        vec![inbox()],
        vec![
            mail("h8", "inbox", "Quarterly report", 8),
            mail("h9", "inbox", "Quarterly report", 9),
            mail("h10", "inbox", "Quarterly report", 10),
        ],
    );
    seed(&engine, &provider, &account()).await;

    let first = messages::search(&engine, &[account()], false, None, "report", 0, 2)
        .await
        .unwrap();
    assert_eq!(row_keys(&first), vec!["h10", "h9"]);
    assert_eq!(first["total"], 3);
    assert_eq!(first["has_more"], true);

    let second = messages::search(&engine, &[account()], false, None, "report", 2, 2)
        .await
        .unwrap();
    assert_eq!(row_keys(&second), vec!["h8"]);
    assert_eq!(second["has_more"], false);
}

#[tokio::test]
async fn a_query_of_nothing_answers_an_empty_page() {
    let engine = seeded_accounts().await;

    let answer = messages::search(&engine, &[account()], false, None, "   ", 0, 20)
        .await
        .unwrap();
    assert_eq!(answer["rows"].as_array().unwrap().len(), 0);
    assert_eq!(answer["total"], 0);
    assert_eq!(answer["has_more"], false);
}

#[tokio::test]
async fn a_malformed_query_is_a_usage_error() {
    let engine = seeded_accounts().await;

    let answer =
        messages::search(&engine, &[account()], false, None, "subject:\"open", 0, 20).await;
    assert_eq!(answer.unwrap_err().0.exit(), 2);
}

#[test]
fn the_run_layer_answers_the_search_envelope_and_audits_shapes_only() {
    let dir = super::fakes::temp_dir();
    super::fakes::write_index(&dir, &["alpha"]);
    // `run` builds its own runtime, so the seeding rides a separate one.
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let engine = Engine::open(dir.join(super::ENGINE_STORE_FILE)).unwrap();
        let provider = FakeProvider::with(
            vec![inbox()],
            vec![mail("m1", "inbox", "Quarterly report", 8)],
        );
        seed(&engine, &provider, &account()).await;
    });

    let outcome = super::run(
        &[
            "messages".to_owned(),
            "search".to_owned(),
            "--json".to_owned(),
            "--query".to_owned(),
            "report".to_owned(),
            "--account".to_owned(),
            "alpha".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let data = outcome.envelope.data().unwrap();
    assert_eq!(data["total"], 1);
    assert_eq!(data["rows"][0]["key"], "m1");
    assert_eq!(data["has_more"], false);

    let conn = rusqlite::Connection::open(dir.join(super::AI_STORE_FILE)).unwrap();
    let mut statement = conn
        .prepare("SELECT verb, arg_preview, outcome FROM audit_log ORDER BY rowid")
        .unwrap();
    let rows: Vec<(String, String, String)> = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "messages search");
    assert_eq!(rows[0].1, "query_chars=6 account=alpha limit=20 offset=0");
    assert_eq!(rows[0].2, "ok");
    assert!(
        !rows[0].1.contains("report"),
        "the ledger holds the query's length, never its text"
    );
}

#[test]
fn search_usage_failures_pin_the_scope_and_bounds_rules() {
    let dir = super::fakes::temp_dir();
    super::fakes::write_index(&dir, &["alpha"]);

    let folder_without_account = super::run(
        &[
            "messages".to_owned(),
            "search".to_owned(),
            "--json".to_owned(),
            "--query".to_owned(),
            "report".to_owned(),
            "--folder".to_owned(),
            "inbox".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(
        folder_without_account.code.exit(),
        2,
        "a folder without its account is a usage error"
    );

    let overlarge_limit = super::run(
        &[
            "messages".to_owned(),
            "search".to_owned(),
            "--json".to_owned(),
            "--query".to_owned(),
            "report".to_owned(),
            "--limit".to_owned(),
            "51".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(
        overlarge_limit.code.exit(),
        2,
        "the page bound is the contract's own"
    );
}
