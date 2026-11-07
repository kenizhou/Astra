// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the outgoing name per account: the join's adoption over each
//! identity family (writable, read-only, none), the settings family's set
//! arm (store first, push best-effort, the refusal kept and cleared), the
//! ask-once state, and the From line the stored name rides into.

use std::sync::{Arc, Mutex};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use astra_viewmodel::SenderNameControl;
use engine_api::{AccountId, EmailAddress, SenderIdentity, SenderIdentityId};

use crate::{dispatch_fakes as fakes, protocol::SettingsIntent, App, Intent};

/// A well-formed account id, the way the sender-name tests name one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// The identity a provider holds for the test account's own address
/// (`me@work.local`), named or not the way a fresh mailbox is not.
fn own_identity(name: Option<&str>) -> SenderIdentity {
    let address = match name {
        Some(name) => EmailAddress::named(name, "me@work.local"),
        None => EmailAddress::new("me@work.local"),
    };
    SenderIdentity::new(SenderIdentityId::new("identity-1"), address)
}

/// A one-paragraph document carrying `text`, the smallest document a send
/// holds.
fn document(text: &str) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![Block::Paragraph(Paragraph {
            content: vec![InlineContent::Text(TextRun {
                text: text.to_owned(),
                bold: false,
                italic: false,
                underline: false,
                font_size: None,
                color: None,
                highlight: None,
                link: None,
            })],
        })],
        attachments: Vec::new(),
    }
}

/// Drives the settings family's sender-name arm.
async fn set_name(app: &App<fakes::FakeProvider>, id: &str, name: &str) {
    app.dispatch(Intent::Settings(SettingsIntent::SetSenderName {
        account: account(id),
        name: name.to_owned(),
    }))
    .await
    .unwrap();
}

/// Republishes the settings surface, so a test reads the state rather than
/// the publish.
async fn refresh(app: &App<fakes::FakeProvider>) {
    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_writable_provider_s_name_is_adopted_at_the_join() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .with_writable_identities(vec![own_identity(Some("Ada Lovelace"))]);
    let writes = provider.name_writes();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.sync_added_account(&account("work")).await;
    refresh(&app).await;

    let row = &app.settings().accounts[0];
    assert_eq!(row.display_name, "Ada Lovelace");
    assert!(!row.needs_sender_name, "the adoption answers the ask");
    assert_eq!(row.sender_name_control, SenderNameControl::Writable);
    assert!(
        writes.lock().unwrap().is_empty(),
        "the adoption is local only: the name came from the provider"
    );
}

#[tokio::test]
async fn a_read_only_provider_s_name_is_adopted_and_the_row_states_it() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .with_read_only_identities(vec![own_identity(Some("Directory Name"))]);
    let writes = provider.name_writes();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.sync_added_account(&account("work")).await;
    refresh(&app).await;

    let row = &app.settings().accounts[0];
    assert_eq!(row.display_name, "Directory Name");
    assert!(!row.needs_sender_name);
    assert_eq!(
        row.sender_name_control,
        SenderNameControl::ReadOnly,
        "the card shows the name and offers no field"
    );
    assert!(writes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_provider_with_no_identity_object_leaves_the_ask_standing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.sync_added_account(&account("work")).await;
    refresh(&app).await;

    let row = &app.settings().accounts[0];
    assert_eq!(
        row.display_name, "me@work.local",
        "the row falls back to the address"
    );
    assert!(
        row.needs_sender_name,
        "the IMAP shape owns no name to adopt"
    );
    assert_eq!(row.sender_name_control, SenderNameControl::Local);
}

#[tokio::test]
async fn a_provider_answering_no_name_leaves_the_ask_standing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .with_writable_identities(vec![own_identity(None)]);
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.sync_added_account(&account("work")).await;
    refresh(&app).await;

    let row = &app.settings().accounts[0];
    assert!(
        row.needs_sender_name,
        "a fresh mailbox is exactly the ask's case"
    );
    assert_eq!(row.display_name, "me@work.local");
}

#[tokio::test]
async fn setting_the_name_stores_it_and_pushes_the_provider_s_copy() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .with_writable_identities(vec![own_identity(Some("Old Name"))]);
    let writes = provider.name_writes();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    set_name(&app, "work", "Grace Hopper").await;

    let row = &app.settings().accounts[0];
    assert_eq!(row.display_name, "Grace Hopper");
    assert!(!row.needs_sender_name);
    assert_eq!(row.sender_name_error, None);
    let writes = writes.lock().unwrap();
    assert_eq!(writes.len(), 1, "the push names the matching identity");
    assert_eq!(writes[0].0.as_str(), "identity-1");
    assert_eq!(writes[0].1, "Grace Hopper");
}

#[tokio::test]
async fn a_refused_push_keeps_the_local_name_and_shows_then_clears_the_sentence() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .with_writable_identities(vec![own_identity(Some("Old Name"))])
        .refusing_name_writes(1);
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    set_name(&app, "work", "Grace Hopper").await;
    let row = &app.settings().accounts[0];
    assert_eq!(
        row.display_name, "Grace Hopper",
        "the local name stands: it is the From line's source"
    );
    assert!(
        row.sender_name_error
            .as_deref()
            .is_some_and(|sentence| !sentence.is_empty()),
        "the refusal lands on the row"
    );

    // The next write lands (the script holds one refusal), and the standing
    // sentence goes with it.
    set_name(&app, "work", "Grace Brewster Hopper").await;
    let row = &app.settings().accounts[0];
    assert_eq!(row.display_name, "Grace Brewster Hopper");
    assert_eq!(row.sender_name_error, None);
}

#[tokio::test]
async fn a_name_past_the_engine_s_bound_is_refused_at_the_setting() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .with_writable_identities(vec![own_identity(Some("Old Name"))]);
    let writes = provider.name_writes();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    set_name(&app, "work", &"a".repeat(129)).await;

    let row = &app.settings().accounts[0];
    assert_eq!(
        row.display_name, "me@work.local",
        "nothing is stored past the bound"
    );
    let sentence = row.sender_name_error.as_deref().unwrap();
    assert!(sentence.contains("128"), "the bound is named: {sentence}");
    assert!(writes.lock().unwrap().is_empty(), "no provider is asked");
}

#[tokio::test]
async fn a_control_character_name_is_refused_without_echoing_it() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .with_writable_identities(vec![own_identity(Some("Old Name"))]);
    let writes = provider.name_writes();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    set_name(&app, "work", "Ada\r\nBcc: eve@example.test").await;

    let row = &app.settings().accounts[0];
    assert_eq!(row.display_name, "me@work.local", "nothing is stored");
    let sentence = row.sender_name_error.as_deref().unwrap();
    assert!(
        !sentence.contains("eve@example.test") && !sentence.contains('\r'),
        "the refusal never carries the payload onward: {sentence}"
    );
    assert!(writes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_empty_name_is_a_real_answer_and_means_the_bare_address() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);
    assert!(app.needs_sender_name(&account("work")));

    set_name(&app, "work", "").await;

    let row = &app.settings().accounts[0];
    assert!(
        !row.needs_sender_name,
        "the ask is spent, not still standing"
    );
    assert_eq!(row.display_name, "me@work.local");
    assert!(!app.needs_sender_name(&account("work")));
}

#[tokio::test]
async fn a_read_only_provider_s_name_is_stored_locally_and_never_pushed() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .with_read_only_identities(vec![own_identity(Some("Directory Name"))]);
    let writes = provider.name_writes();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    set_name(&app, "work", "Grace Hopper").await;

    let row = &app.settings().accounts[0];
    assert_eq!(row.display_name, "Grace Hopper");
    assert_eq!(
        row.sender_name_error, None,
        "nothing was refused: no write was the provider's to take"
    );
    assert!(writes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_from_line_carries_the_stored_name() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    set_name(&app, "work", "Grace Hopper").await;
    app.dispatch(Intent::SubmitRichMail {
        from: account("work"),
        to: "you@test.local".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "the name goes out".to_owned(),
        document: document("On its way"),
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    })
    .await
    .unwrap();

    let submits = submits.lock().unwrap();
    let from = &submits[0].from;
    assert_eq!(from.name.as_deref(), Some("Grace Hopper"));
    assert_eq!(from.email, "me@work.local");
}

#[tokio::test]
async fn without_a_stored_name_the_from_line_is_the_bare_address() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.dispatch(Intent::SubmitRichMail {
        from: account("work"),
        to: "you@test.local".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "the bare address".to_owned(),
        document: document("On its way"),
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    })
    .await
    .unwrap();

    let submits = submits.lock().unwrap();
    assert_eq!(submits[0].from.name, None);
    assert_eq!(submits[0].from.email, "me@work.local");
}

#[tokio::test]
async fn a_stored_name_survives_the_boot_s_re_adoption() {
    // The slow join path runs at every launch for every stored account:
    // a typed answer standing over the provider's stale copy is the
    // answer that must survive the restart, so the adoption stands down
    // wherever a name is already stored.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .with_writable_identities(vec![own_identity(Some("Provider Old"))]);
    let writes = provider.name_writes();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    set_name(&app, "work", "Grace Hopper").await;

    // The boot's shape: the join path re-runs over the standing answer.
    app.sync_added_account(&account("work")).await;
    refresh(&app).await;

    let row = &app.settings().accounts[0];
    assert_eq!(
        row.display_name, "Grace Hopper",
        "the typed name stands; the provider's stale copy does not revert it"
    );
    let writes = writes.lock().unwrap();
    assert_eq!(
        writes.len(),
        1,
        "only the set's own push: the re-adoption wrote nothing and pushed nothing"
    );
    assert_eq!(writes[0].1, "Grace Hopper");
}

#[tokio::test]
async fn the_pane_rows_carry_the_name_the_from_line_carries() {
    // The snapshot's account rows are the From picker's and the folder
    // pane's one source: the stored name is the label both render, and
    // the address answers only while no name stands.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(
        vec![fakes::account(
            "work",
            fakes::FakeProvider::with(vec![fakes::inbox()]),
        )],
        &surfaces,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();

    assert_eq!(
        app.mailbox_list().account_folders[0].display_name,
        "me@work.local",
        "an unnamed account's label is its address"
    );

    set_name(&app, "work", "Grace Hopper").await;

    assert_eq!(
        app.mailbox_list().account_folders[0].display_name,
        "Grace Hopper",
        "the stored name is the pane row's label"
    );
}
