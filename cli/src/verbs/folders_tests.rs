// SPDX-License-Identifier: GPL-3.0-only
//! `folders list` tests, over an in-memory engine seeded the way a first
//! sync would fill it.

use engine_api::{AccountId, Engine};

use super::fakes::{folder, inbox, seed, FakeProvider};
use super::folders;

fn account() -> AccountId {
    AccountId::try_from("alpha").unwrap()
}

#[tokio::test]
async fn folders_list_answers_the_tree_with_roles_and_unread_counts() {
    let engine = Engine::open_in_memory().unwrap();
    let mut drafts = folder("drafts", "Drafts");
    drafts.role = Some(engine_api::MailboxRole::Drafts);
    drafts.unread_count = Some(2);
    let provider = FakeProvider::with(vec![inbox(), drafts], Vec::new());
    seed(&engine, &provider, &account()).await;

    let answer = folders::list(&engine, &account()).await.unwrap();
    assert_eq!(answer["account"], "alpha");
    let list = answer["folders"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], "inbox");
    assert_eq!(list[0]["name"], "Inbox");
    assert_eq!(list[0]["role"], "inbox");
    assert_eq!(list[1]["role"], "drafts");
    assert_eq!(list[1]["unread_count"], 2);
}

#[tokio::test]
async fn folders_list_answers_an_empty_tree_for_an_unsynced_account() {
    let engine = Engine::open_in_memory().unwrap();
    let answer = folders::list(&engine, &account()).await.unwrap();
    assert_eq!(answer["folders"], serde_json::json!([]));
}

#[tokio::test]
async fn a_roleless_folder_carries_null_role_and_parent() {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(vec![folder("archive", "Archive")], Vec::new());
    seed(&engine, &provider, &account()).await;
    let answer = folders::list(&engine, &account()).await.unwrap();
    let folder = &answer["folders"][0];
    assert_eq!(folder["role"], serde_json::Value::Null);
    assert_eq!(folder["parent"], serde_json::Value::Null);
}
