// SPDX-License-Identifier: GPL-3.0-only
//! `messages mark-read`, `set-flagged`, `archive`, and `trash` tests: the
//! engine edit each verb carries, the parked op the offline provider
//! answers, the preview's equality with the edit a run actually puts, and
//! the whole run layer's envelope and audit over a seeded file store.

use engine_api::{AccountId, Engine, MailEdit, MailboxRole, ProviderKey};

use super::fakes::{inbox, mail, role_folder, seed, temp_dir, write_index, FakeProvider};
use super::{message_writes, Offline};

fn account() -> AccountId {
    AccountId::try_from("alpha").unwrap()
}

fn key() -> ProviderKey {
    ProviderKey::new("m1").unwrap()
}

/// The tree every move verb resolves its destination from: a role-tagged
/// archive and trash beside the inbox.
fn tree_with_targets() -> Vec<engine_api::Mailbox> {
    vec![
        inbox(),
        role_folder("arch", "Archive", MailboxRole::Archive),
        role_folder("bin", "Trash", MailboxRole::Trash),
    ]
}

async fn seeded(tree: Vec<engine_api::Mailbox>) -> (Engine, FakeProvider) {
    let engine = Engine::open_in_memory().unwrap();
    let provider = FakeProvider::with(tree, vec![mail("m1", "inbox", "Quarterly report", 8)]);
    seed(&engine, &provider, &account()).await;
    (engine, provider)
}

#[tokio::test]
async fn mark_read_carries_the_seen_edit_and_answers_applied() {
    let (engine, provider) = seeded(tree_with_targets()).await;

    let answer = message_writes::mark_read(&engine, &provider, &account(), "m1", true, false)
        .await
        .unwrap();
    assert_eq!(answer["key"], "m1");
    assert_eq!(answer["state"], "applied");
    assert!(answer["op"].as_u64().unwrap() > 0, "the durable op rides");
    assert_eq!(
        provider.mail_edits(),
        vec![MailEdit::mark_seen(key(), true)]
    );
}

#[tokio::test]
async fn mark_read_unread_carries_the_clearing_edit() {
    let (engine, provider) = seeded(tree_with_targets()).await;

    message_writes::mark_read(&engine, &provider, &account(), "m1", false, false)
        .await
        .unwrap();
    assert_eq!(
        provider.mail_edits(),
        vec![MailEdit::mark_seen(key(), false)]
    );
}

#[tokio::test]
async fn set_flagged_carries_the_flag_edit_both_ways() {
    let (engine, provider) = seeded(tree_with_targets()).await;

    message_writes::set_flagged(&engine, &provider, &account(), "m1", true, false)
        .await
        .unwrap();
    message_writes::set_flagged(&engine, &provider, &account(), "m1", false, false)
        .await
        .unwrap();
    assert_eq!(
        provider.mail_edits(),
        vec![
            MailEdit::set_flagged(key(), true),
            MailEdit::set_flagged(key(), false),
        ]
    );
}

#[tokio::test]
async fn archive_moves_to_the_account_archive_folder() {
    let (engine, provider) = seeded(tree_with_targets()).await;

    let answer = message_writes::archive(&engine, &provider, &account(), "m1", false)
        .await
        .unwrap();
    assert_eq!(answer["state"], "applied");
    let destination = engine_api::MailboxId::try_from("arch").unwrap();
    assert_eq!(
        provider.mail_edits(),
        vec![MailEdit::move_to(key(), destination)]
    );
}

#[tokio::test]
async fn trash_moves_to_the_account_trash_folder() {
    let (engine, provider) = seeded(tree_with_targets()).await;

    message_writes::trash(&engine, &provider, &account(), "m1", false)
        .await
        .unwrap();
    let destination = engine_api::MailboxId::try_from("bin").unwrap();
    assert_eq!(
        provider.mail_edits(),
        vec![MailEdit::move_to(key(), destination)]
    );
}

#[tokio::test]
async fn a_move_resolves_a_conventionally_named_destination() {
    let (engine, provider) = seeded(vec![inbox(), super::fakes::folder("kept", "Archives")]).await;

    message_writes::archive(&engine, &provider, &account(), "m1", false)
        .await
        .unwrap();
    let destination = engine_api::MailboxId::try_from("kept").unwrap();
    assert_eq!(
        provider.mail_edits(),
        vec![MailEdit::move_to(key(), destination)]
    );
}

#[tokio::test]
async fn an_archive_falls_back_to_the_all_role_folder() {
    // The third tier the app side also pins: an account whose only archive
    // home is the All-rolled-up folder must move there, not fail.
    let (engine, provider) = seeded(vec![
        inbox(),
        role_folder("all", "All Mail", MailboxRole::All),
    ])
    .await;

    message_writes::archive(&engine, &provider, &account(), "m1", false)
        .await
        .unwrap();
    let destination = engine_api::MailboxId::try_from("all").unwrap();
    assert_eq!(
        provider.mail_edits(),
        vec![MailEdit::move_to(key(), destination)]
    );
}

#[tokio::test]
async fn a_move_with_no_destination_folder_is_a_runtime_error() {
    let (engine, provider) = seeded(vec![inbox()]).await;

    let answer = message_writes::archive(&engine, &provider, &account(), "m1", false).await;
    let (code, message) = answer.unwrap_err();
    assert_eq!(code.exit(), 1);
    assert!(
        message.contains("no archive"),
        "the failure names the missing folder: {message}"
    );
    assert!(provider.mail_edits().is_empty(), "nothing was queued");
}

#[tokio::test]
async fn the_act_verbs_park_the_op_under_the_offline_provider() {
    let (engine, provider) = seeded(tree_with_targets()).await;
    let before = provider.mail_edits().len();

    let answer = message_writes::mark_read(&engine, &Offline, &account(), "m1", true, false)
        .await
        .unwrap();
    assert_eq!(answer["state"], "queued", "the durable op is the answer");
    assert!(answer["op"].as_u64().unwrap() > 0);
    let rows = engine.outbox(&account()).await.unwrap();
    assert!(rows.iter().any(|row| {
        row.kind == Some(engine_api::PendingOpKind::MailEdit)
            && row.resource_key.as_str() == "mail:m1"
    }));
    assert_eq!(provider.mail_edits().len(), before, "no provider leg ran");
}

#[tokio::test]
async fn the_preview_prints_the_exact_edit_a_run_carries() {
    let (engine, provider) = seeded(tree_with_targets()).await;

    let preview = message_writes::mark_read(&engine, &provider, &account(), "m1", true, true)
        .await
        .unwrap();
    assert_eq!(preview["action"], "edit_mail");
    assert_eq!(preview["account"], "alpha");
    assert_eq!(preview["key"], "m1");
    assert_eq!(
        preview["edit"],
        serde_json::to_value(MailEdit::mark_seen(key(), true)).unwrap()
    );
    assert!(
        !preview.to_string().contains("Quarterly"),
        "the preview never echoes message text"
    );

    // Equality against what the engine call actually carried: the same
    // builder feeds both, so the edit must agree one for one.
    message_writes::mark_read(&engine, &provider, &account(), "m1", true, false)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(provider.mail_edits()[0].clone()).unwrap(),
        preview["edit"]
    );
    assert!(provider.mail_edits().len() == 1, "a preview runs nothing");
}

#[tokio::test]
async fn the_move_preview_names_the_resolved_destination() {
    let (engine, provider) = seeded(tree_with_targets()).await;

    let preview = message_writes::archive(&engine, &provider, &account(), "m1", true)
        .await
        .unwrap();
    assert_eq!(
        preview["edit"],
        serde_json::to_value(MailEdit::move_to(
            key(),
            engine_api::MailboxId::try_from("arch").unwrap()
        ))
        .unwrap()
    );
    assert!(provider.mail_edits().is_empty(), "a preview runs nothing");
}

#[tokio::test]
async fn a_key_naming_no_message_is_a_runtime_error() {
    let (engine, provider) = seeded(tree_with_targets()).await;

    let answer =
        message_writes::mark_read(&engine, &provider, &account(), "absent", true, false).await;
    let (code, message) = answer.unwrap_err();
    assert_eq!(code.exit(), 1);
    assert!(
        message.contains("no message"),
        "the failure names the missing message: {message}"
    );
    assert_eq!(provider.mail_edits(), [] as [engine_api::MailEdit; 0]);
}

#[tokio::test]
async fn a_malformed_key_is_a_usage_error() {
    let (engine, provider) = seeded(tree_with_targets()).await;

    let answer = message_writes::mark_read(&engine, &provider, &account(), "", true, false).await;
    assert_eq!(answer.unwrap_err().0.exit(), 2);
}

/// Seeds the run layer's own file store, the one a real invocation opens.
/// `run` builds its own runtime, so the seeding rides a separate one.
fn seed_file_store(dir: &std::path::Path) {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async move {
            let engine = Engine::open(dir.join(super::ENGINE_STORE_FILE)).unwrap();
            let provider = FakeProvider::with(
                tree_with_targets(),
                vec![mail("m1", "inbox", "Quarterly report", 8)],
            );
            seed(&engine, &provider, &account()).await;
        });
}

/// The audit ledger's rows as `(verb, arg_preview, outcome)`, read straight
/// from the ledger file the run wrote.
fn audit_rows(dir: &std::path::Path) -> Vec<(String, String, String)> {
    let conn = rusqlite::Connection::open(dir.join(super::AI_STORE_FILE)).unwrap();
    let mut statement = conn
        .prepare("SELECT verb, arg_preview, outcome FROM audit_log ORDER BY rowid")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn the_run_layer_previews_the_act_verb_and_audits_the_preview() {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    seed_file_store(&dir);

    let outcome = super::run(
        &[
            "messages".to_owned(),
            "mark-read".to_owned(),
            "--json".to_owned(),
            "--account".to_owned(),
            "alpha".to_owned(),
            "--key".to_owned(),
            "m1".to_owned(),
            "--preview".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    let data = outcome.envelope.data().unwrap();
    assert_eq!(data["action"], "edit_mail");
    assert_eq!(
        data["edit"]["SetKeywords"]["add"],
        serde_json::json!(["$seen"])
    );
    let rows = audit_rows(&dir);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "messages mark-read");
    assert_eq!(rows[0].1, "account=alpha key=m1 read=true");
    assert_eq!(rows[0].2, "preview");
    assert!(
        !rows[0].1.contains("Quarterly"),
        "the ledger never holds message text"
    );
}

#[test]
fn the_run_layer_queues_the_act_verb_under_the_offline_provider() {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    seed_file_store(&dir);

    let outcome = super::run(
        &[
            "messages".to_owned(),
            "trash".to_owned(),
            "--json".to_owned(),
            "--account".to_owned(),
            "alpha".to_owned(),
            "--key".to_owned(),
            "m1".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 0);
    assert_eq!(outcome.envelope.data().unwrap()["state"], "queued");
    assert_eq!(audit_rows(&dir)[0].0, "messages trash");
    assert_eq!(audit_rows(&dir)[0].2, "ok");
}

#[test]
fn act_verb_usage_failures_audit_as_usage() {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);

    let outcome = super::run(
        &[
            "messages".to_owned(),
            "mark-read".to_owned(),
            "--json".to_owned(),
            "--account".to_owned(),
            "alpha".to_owned(),
        ],
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 2, "the missing --key is a usage error");
    let rows = audit_rows(&dir);
    assert_eq!(rows[0].0, "messages mark-read");
    assert_eq!(rows[0].2, "usage");
}
