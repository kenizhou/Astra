// SPDX-License-Identifier: GPL-3.0-only
//! The gated warm proof against the live harness: the per-account body
//! cap's states over the account's own passes, the seeded notes the
//! corpus.
//!
//! An off account keeps its in-window mail as metadata alone (the
//! missing-body read names the notes), a byte cap's pass warms them until
//! the same read empties and a reading open answers from the store's
//! copy, and a return to off drops the cached sources again with the
//! reading pipeline still opening the words. Every pass drives the app's
//! own warm path after the sync, exactly as every pass does; the proof
//! only sets the cap and runs the passes.
//!
//! The proof opens a seeded note and puts it back to unread at the end
//! through the product's own write path, the suites' standing citizenship.

use std::sync::Arc;

use astra_viewmodel::RowKey;
use engine_api::{AccountId, EmailAddress, Engine, Provider, ProviderKey};

use crate::protocol::SettingsIntent;
use crate::{reference::MessageRef, Account, App, AppObserver, Intent, SyncBodyCap};

use super::{dial_with_accepted_certificate, harness, mark_read, row_by_subject, CountingObserver};

/// The seeded note the proof warms and opens.
const NOTE: &str = "Harness note one";

/// The note's body words, the seed's own.
const NOTE_WORDS: &str = "First note delivered through the harness submission port.";

/// The per-message cap the proof sets: generous for a note, still a bound.
const CAP_OCTETS: u64 = 64 * 1024;

/// The account's stored rows still missing their cached bodies, the
/// store-side truth the warm pass empties and the off cap refills.
async fn missing_bodies(app: &App<Box<dyn Provider>>, account: &AccountId) -> Vec<ProviderKey> {
    app.engine
        .mail_missing_body(
            std::slice::from_ref(account),
            crate::body_warm::WARM_WORK_LIMIT,
        )
        .await
        .expect("the missing-body read")
        .into_iter()
        .map(|row| row.mail.key)
        .collect()
}

/// Writes the account's body cap through the settings intent, the editor's
/// own gesture.
async fn set_cap(app: &App<Box<dyn Provider>>, account: &AccountId, cap: SyncBodyCap) {
    app.dispatch(Intent::Settings(SettingsIntent::SetSyncSettings {
        account: account.clone(),
        depth_months: None,
        body_cap: Some(cap),
        poll_minutes: None,
    }))
    .await
    .unwrap();
    assert!(
        app.settings().error.is_none(),
        "the cap write lands without error"
    );
}

/// The list row's key for `subject`, over the product's own list.
fn note_key(app: &App<Box<dyn Provider>>, subject: &str) -> ProviderKey {
    let RowKey::Message { key, .. } = row_by_subject(&app.mailbox_list(), subject).key.clone()
    else {
        panic!("a flat list row is keyed by its message");
    };
    key
}

/// Opens `subject` through the reading pipeline and pins the seed's own
/// words: the pipeline the warmed body serves from the store and the wire
/// re-fetches alike. The open closes again so a later open can stand.
async fn open_note(app: &App<Box<dyn Provider>>, subject: &str) {
    let RowKey::Message { account, key } = row_by_subject(&app.mailbox_list(), subject).key.clone()
    else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::OpenMessage {
        message: MessageRef { account, key },
    })
    .await
    .unwrap();
    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert_eq!(reading.subject, subject);
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(NOTE_WORDS)),
        "the opened body holds the seed's words: {:?}",
        reading.plain
    );
    app.dispatch(Intent::CloseReading).await.unwrap();
}

/// The warm proof against the live harness: the off account holds its
/// in-window mail as metadata alone, the byte cap's pass warms it until
/// the missing-body read empties, and the return to off drops the cached
/// sources with the reading pipeline still answering.
#[tokio::test]
async fn the_warm_pass_fills_under_a_cap_and_off_drops_the_cached_sources() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (account_id, providers, ..) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account_id.clone(),
            providers,
            identity: EmailAddress::new("astra1@example.test"),
        }],
        None,
        observer,
    );

    // The metadata-only state, set before the first pass imports anything,
    // so the notes land as rows alone.
    set_cap(&app, &account_id, SyncBodyCap::Off).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let missing = missing_bodies(&app, &account_id).await;
    let key = note_key(&app, NOTE);
    assert!(
        missing.contains(&key),
        "the off account keeps its in-window mail as metadata alone: {missing:?}"
    );
    // The open still answers under off: the body rides the wire.
    open_note(&app, NOTE).await;

    // The byte cap's pass warms: the app's own warm path, driven by the
    // pass exactly as every pass drives it, until no in-window mail is
    // missing its body.
    set_cap(&app, &account_id, SyncBodyCap::Bytes { octets: CAP_OCTETS }).await;
    app.sync_account(&account_id).await;
    let missing = missing_bodies(&app, &account_id).await;
    assert!(
        missing.is_empty(),
        "the warmed in-window mail holds its bodies in the store: {missing:?}"
    );
    // The warmed body opens through the reading pipeline, served from the
    // store the missing read just proved full.
    open_note(&app, NOTE).await;

    // Back to off: the cached sources drop, and the reading pipeline still
    // opens the note's words.
    set_cap(&app, &account_id, SyncBodyCap::Off).await;
    app.sync_account(&account_id).await;
    let missing = missing_bodies(&app, &account_id).await;
    let key = note_key(&app, NOTE);
    assert!(
        missing.contains(&key),
        "the off cap drops the cached sources: {missing:?}"
    );
    open_note(&app, NOTE).await;

    // Citizenship: the opened note back to unread through the product's
    // own write path, and a pass to land the write.
    mark_read(&app, NOTE, false).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
