// SPDX-License-Identifier: GPL-3.0-only
//! The gated list and dial tests against the live harness: the dial that
//! walks the certificate acceptance path and binds the seeded folders, the
//! message list the three seeded notes fill after one refresh with a
//! mark-read that survives a further refresh, and the detection run that
//! routes a harness user onto the JMAP form. The first suites the harness
//! grew; they read the shared fixtures the root file holds.

use std::{sync::Arc, time::Duration};

use astra_autodetect::{detect, DetectConfig, Detected, SourceKind};
use engine_api::{AccountId, EmailAddress, Engine, Provider, SyncScope};

use crate::{Account, App, AppObserver, FolderRef, Intent};

use super::{
    dial_with_accepted_certificate, harness, inbox_key, jmap_base, row_by_subject,
    CountingObserver, NOTES,
};

/// The mailbox each provider is bound to, in the order the dial returned them.
fn bound_mailboxes(providers: &[Box<dyn Provider>], account_id: &AccountId) -> Vec<String> {
    providers
        .iter()
        .map(|provider| match provider.email_scope(account_id) {
            SyncScope::ImapMailbox { mailbox, .. } => mailbox.as_str().to_owned(),
            other => panic!("an IMAP folder provider bound to {other:?}"),
        })
        .collect()
}

/// How many listed rows are unread.
fn unread_count(snapshot: &astra_viewmodel::MailboxListSnapshot) -> usize {
    snapshot.rows.iter().filter(|row| row.unread).count()
}

/// The dial against the harness walks the certificate acceptance path a person does:
/// refused first, the rejection recorded, the one certificate accepted for the one
/// server, and then the dial binds the INBOX and the custom folder the seed created.
#[tokio::test]
async fn a_dial_pins_the_rejected_certificate_and_binds_the_inbox_and_the_custom_folder() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (account_id, providers, ..) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;

    let bound = bound_mailboxes(&providers, &account_id);
    assert_eq!(
        bound.first().map(String::as_str),
        Some("INBOX"),
        "the reserved INBOX is bound first; the bound list is {bound:?}",
    );
    assert!(
        bound.contains(&"Projects".to_owned()),
        "the custom folder astra1 seeds is bound; the bound list is {bound:?}",
    );
    // The submission leg rides the same walk: both legs name the one server whose
    // certificate was just accepted, and every provider the dial returns carries the
    // SMTP binding the outbox drain delivers through. A send queued against this
    // account has a provider that can take it.
    assert!(
        providers
            .iter()
            .all(|provider| provider.connection_info().capabilities.submission()),
        "the dial bound no submission provider beside the folders",
    );
}

/// The message list against the live harness: the three notes the seed delivers
/// through the harness's own SMTP submission appear in astra1's INBOX after one
/// refresh, newest first, and a mark-read writes through to the server: the row flips
/// at once, and the read state survives a further refresh.
///
/// The run builds the loop the desktop host builds: the engine in a scratch store,
/// the accepted dial's providers, the app over them. `dispatch` awaits each whole
/// pass (sync, outbox drain, rebuild), so the snapshot is final and needs no polling.
///
/// A warm volume may hold the read state an earlier run left, so the test first puts
/// every note back to unread through the product's own write path; the flip is then
/// proven from a known state on any volume.
#[tokio::test]
async fn the_message_list_shows_the_seeded_notes_and_a_mark_read_survives_a_refresh() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (account_id, providers, ..) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;

    let counter = Arc::new(CountingObserver::default());
    let observer: Arc<dyn AppObserver> = counter.clone();
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

    app.dispatch(Intent::RefreshMail).await.unwrap();

    let inbox = FolderRef::from_parts(account_id.as_str(), inbox_key(&app, &account_id))
        .expect("the INBOX folder reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();

    // The three seeded notes, newest first: the seed pins the Date headers and
    // delivers oldest first, so the order holds whatever clock served them.
    let snapshot = app.mailbox_list();
    let subjects: Vec<&str> = snapshot
        .rows
        .iter()
        .map(|row| row.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        ["Harness note three", "Harness note two", "Harness note one"],
        "the whole INBOX is the three seeded notes, newest first",
    );
    let one = row_by_subject(&snapshot, "Harness note one");
    assert_eq!(one.sender_name, "Astra Two");
    assert_eq!(one.sender_address, "astra2@example.test");

    // Back to a known state: every note unread, the writes reaching the server on the
    // same pass (the refresh drains the outbox after the sync).
    for subject in NOTES {
        super::mark_read(&app, subject, false).await;
    }
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let snapshot = app.mailbox_list();
    assert_eq!(
        unread_count(&snapshot),
        NOTES.len(),
        "every note is unread after the reset pass"
    );

    // The flip: one row marked read flips at once and republishes the list.
    let signals = counter.signals();
    super::mark_read(&app, "Harness note two", true).await;
    assert!(
        counter.signals() > signals,
        "the flipped list is republished"
    );
    let snapshot = app.mailbox_list();
    assert!(
        !row_by_subject(&snapshot, "Harness note two").unread,
        "the marked row flipped"
    );
    assert_eq!(unread_count(&snapshot), 2, "two notes are still unread");

    // And the write holds: a further refresh syncs the server's state back and the
    // row stays read.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        !row_by_subject(&snapshot, "Harness note two").unread,
        "the read state survives a refresh"
    );
    assert_eq!(
        unread_count(&snapshot),
        2,
        "the other two notes are untouched"
    );
}

/// Detection for a harness user runs the real well-known probe over the real HTTP
/// fetcher: `/.well-known/jmap` redirects to the session, the session's anonymous
/// JSON advertises `capabilities`, and the account layer routes the find onto the
/// JMAP form the wizard prefills. The run is clamped to the host command's five
/// second bound, so it also proves the probe fits that budget.
#[tokio::test]
async fn detection_returns_the_jmap_recommendation_for_a_harness_user() {
    let Some(base) = jmap_base() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let config = DetectConfig {
        overall_deadline: Duration::from_secs(5),
        well_known_base_override: Some(url::Url::parse(&base).expect("the harness JMAP URL")),
        ..DetectConfig::default()
    };

    let email = "astra1@example.test";
    let detected = detect(email, None, &config)
        .await
        .expect("the detection run against the harness");

    let Detected::Jmap(jmap) = detected else {
        panic!("the harness domain speaks JMAP over its well-known, got {detected:?}");
    };
    assert_eq!(jmap.base_url, base, "the stored base is the harness origin");
    assert!(
        jmap.is_trusted,
        "the harness override is the sanctioned trusted route"
    );
    assert_eq!(
        jmap.source.kind,
        SourceKind::JmapWellKnown,
        "the apex well-known (rebased) is the winning source"
    );

    let recommendation = astra_account::recommend(
        email,
        Detected::Jmap(jmap),
        astra_account::OauthRoutes {
            google: false,
            microsoft: false,
        },
    );
    let astra_account::SetupRecommendation::Jmap { server_url, .. } = recommendation else {
        panic!("a JMAP find routes to the JMAP form, got {recommendation:?}");
    };
    assert_eq!(
        server_url, base,
        "the JMAP form is prefilled with the harness origin"
    );
}
