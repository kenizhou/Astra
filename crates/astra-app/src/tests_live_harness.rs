// SPDX-License-Identifier: GPL-3.0-only
//! Gated live tests against the local Stalwart harness (docker/harness): the
//! product's IMAP connect path, over real TLS, against a server whose certificate
//! nobody trusts; and the product's detection path, over the real HTTP fetcher,
//! against a server whose domain resolves nowhere.
//!
//! The harness serves implicit TLS on `127.0.0.1:2143` with a self-signed
//! certificate, so the first dial is refused as a certificate rejection and
//! the test pins the accepted certificate exactly as the product's owner
//! would: a [`CertificateException`] recorded for that one server. That is
//! the recorded TLS route for this harness (the decision sits in
//! `docker/harness/compose.yaml` and `scripts/dev/harness.sh`): the account
//! layer dials TLS only. The same certificate serves the SMTP submission
//! listener (`127.0.0.1:2025`), both legs name the one server, so the one
//! accepted exception covers the submission dial too.
//!
//! The seed script (`docker/harness/stalwart/seed.sh`) also delivers three pinned
//! notes to astra1's INBOX through the harness's own SMTP submission port (implicit
//! TLS; curl's `smtps` with `-k`, inside the container, accepting the same self-signed
//! certificate the IMAP seeding does). The notes are genuinely submitted and
//! delivered: Stalwart runs them through its delivery path into the INBOX, exactly the
//! mail the gated list test reads back through the product's own dial and sync. The
//! same seed files one rich MIME note into Projects (submission into the INBOX, then
//! an IMAP move), the message the gated reading test opens in `reading`. The gated
//! drafts test needs no seed: it round-trips the drafts family against the account's
//! real Drafts folder through the product's own dispatches. The gated send test
//! (`send`) needs no seed either: it submits its own mail from astra1 to astra2
//! through the engine's real send path, reads the delivery back, and withdraws both
//! copies so the harness keeps its seeded shape. The gated reply and
//! forward tests (`reply`, `forward`) walk the answer chain over the same wire:
//! each first submits a fresh note from astra1 to astra2 (the seed leaves astra2
//! nothing to answer), then composes the answer through the product's own compose
//! pipeline, submits it as a reply or a forward, and reads the delivery back at
//! astra1 with the threading pinned (`In-Reply-To` matching the original's
//! `Message-ID` for the reply; `References` only, under a `Fwd:` subject, for the
//! forward). Both withdraw every copy they made, so astra1's INBOX ends the run
//! holding the three seeded notes again and astra2 holds nothing. The gated search
//! test (`search`) reads rather than writes: it needs no seed of its own and no
//! cleanup, because every query it runs crosses the engine's real search surface over
//! the corpus the seed already pinned (the three INBOX notes plus the rich note
//! in Projects, so the `from:` hit proves the answer crosses folders), and the
//! harness keeps its seeded shape whatever the queries find. The gated people
//! tests (`people`) extend the same discipline to the CardDAV face: cards are
//! seeded onto each account's default address book through a raw helper under
//! unique run tokens, the dial proof runs the real `connect_dav_providers`,
//! every assertion reads through the product's own paths, and the autosuggest
//! proof submits its own mail so the history it ranks is the run's
//! own, and every seeded card and sent mail is withdrawn at the end (cards
//! through the raw seeding helper: the product ships no contact delete; mail
//! through the product's own delete). The gated settings tests (`settings`)
//! prove the settings editor's account seam live: the dav editor's save dials
//! the real `connect_dav_providers` over a scratch config mirroring the
//! store's merge rule and dial-before-persist ordering, installs both legs,
//! keeps what stood when a re-dial fails, and tears down on the clear; the
//! zone proof pins the now line's zone id.
//!
//! The gated JMAP tests (`jmap`) prove the password account end to end over
//! the harness's plaintext JMAP origin: the dial's session snapshot, the
//! folder list, the seeded notes, a body read, a send round trip with both
//! copies withdrawn, the calendar read the session advertises (seeded over
//! CalDAV, read back over JMAP), and the OAuth gate's fail-soft refusal.
//!
//! The JMAP-detection test cannot dial `https://example.test/.well-known/jmap` (the
//! harness domain resolves nowhere), so it rebases the probe onto the harness's plain
//! HTTP JMAP origin (`127.0.0.1:2080`) through the detection crate's dev-harness
//! `well_known_base_override`, mirroring mail-calendar's live JMAP tests; no
//! host trust store is touched.
//!
//! The tests skip unless `ASTRA_LIVE_HARNESS=1`, so the offline `cargo test` stays
//! green. Against a running harness:
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! cargo test -p astra-app tests_live_harness -- --nocapture --test-threads=1
//! ```
//!
//! The suites share one live server whose contracts pin whole states (the
//! list suite's three-note INBOX; the reply and forward suites deliver into
//! that INBOX before withdrawing their copies), so the recorded invocation
//! runs them one at a time.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use engine_api::{AccountId, EmailAddress, Engine, Provider, SyncScope};

use astra_account::{
    connect_mail_providers, AccountConfig, AccountError, AccountShape, CertificateException,
    ConnectionSecurity, ImapAccount, ImapConnections, Secret, SmtpAccount,
};
use astra_autodetect::{detect, DetectConfig, Detected, SourceKind};
use astra_viewmodel::{FolderRole, MailboxListSnapshot, MessageRowView, RowKey};

use crate::{
    reference::{MessageRef, RowRef},
    Account, App, AppObserver, FolderRef, Intent, Surface,
};

// The gated suites live in their own files, so this one stays under the
// 500-line cap the gate enforces; the explicit `#[path]`s because this file
// owns no directory a bare `mod` would resolve into. `proof` holds the
// fixtures the delivery proofs share.
#[path = "tests_live_harness/calendar.rs"]
mod calendar;
#[path = "tests_live_harness/calendar_brief.rs"]
mod calendar_brief;
#[path = "tests_live_harness/calendar_invite.rs"]
mod calendar_invite;
#[path = "tests_live_harness/calendar_proof.rs"]
mod calendar_proof;
#[path = "tests_live_harness/drafts.rs"]
mod drafts;
#[path = "tests_live_harness/forward.rs"]
mod forward;
#[path = "tests_live_harness/jmap.rs"]
mod jmap;
#[path = "tests_live_harness/people.rs"]
mod people;
#[path = "tests_live_harness/people_proof.rs"]
mod people_proof;
#[path = "tests_live_harness/proof.rs"]
mod proof;
#[path = "tests_live_harness/reading.rs"]
mod reading;
#[path = "tests_live_harness/reply.rs"]
mod reply;
#[path = "tests_live_harness/search.rs"]
mod search;
#[path = "tests_live_harness/send.rs"]
mod send;
#[path = "tests_live_harness/settings.rs"]
mod settings;
#[path = "tests_live_harness/settings_proof.rs"]
mod settings_proof;

/// The harness connection details from the environment, or `None` to skip.
fn harness() -> Option<(String, String, String, String)> {
    if std::env::var("ASTRA_LIVE_HARNESS").ok().as_deref() != Some("1") {
        return None;
    }
    let host = std::env::var("ASTRA_HARNESS_IMAP_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let port = std::env::var("ASTRA_HARNESS_IMAP_PORT").unwrap_or_else(|_| "2143".to_owned());
    let user = std::env::var("ASTRA_HARNESS_IMAP_USER")
        .unwrap_or_else(|_| "astra1@example.test".to_owned());
    let password =
        std::env::var("ASTRA_HARNESS_IMAP_PASSWORD").unwrap_or_else(|_| "astra-harness".to_owned());
    Some((host, port, user, password))
}

/// The harness's plain-HTTP JMAP origin from the environment, or `None` to skip.
fn jmap_base() -> Option<String> {
    if std::env::var("ASTRA_LIVE_HARNESS").ok().as_deref() != Some("1") {
        return None;
    }
    let url = std::env::var("ASTRA_HARNESS_JMAP_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:2080".to_owned());
    Some(url)
}

/// The harness's SMTP submission port from the environment: the compose file publishes
/// Stalwart's implicit-TLS submission listener on `127.0.0.1:2025`.
fn harness_smtp_port() -> String {
    std::env::var("ASTRA_HARNESS_SMTP_PORT").unwrap_or_else(|_| "2025".to_owned())
}

/// The harness account, with the certificate exceptions its owner has accepted (none
/// for the first dial). Both legs name the same server, so one accepted certificate
/// covers the IMAP dial and the SMTP submission alike.
fn account(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
    exceptions: Vec<CertificateException>,
) -> AccountConfig {
    AccountConfig {
        imap: Some(ImapAccount {
            addr: format!("{host}:{port}"),
            server_name: host.to_owned(),
            username: user.to_owned(),
            password: Some(Secret::new(password.to_owned())),
            security: ConnectionSecurity::ImplicitTls,
        }),
        smtp: Some(SmtpAccount {
            addr: format!("{host}:{}", harness_smtp_port()),
            server_name: host.to_owned(),
            security: ConnectionSecurity::ImplicitTls,
        }),
        certificate_exceptions: exceptions,
        graph: None,
        google: None,
        jmap: None,
        dav: None,
        shape: AccountShape::default(),
    }
}

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

/// Dials the harness account through the certificate acceptance path a person's first
/// connect walks: the self-signed certificate is refused, the rejection is recorded,
/// the one certificate is accepted for the one server, and the re-dial returns the
/// account id and every provider it bound (the INBOX first).
async fn dial_with_accepted_certificate(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (AccountId, Vec<Box<dyn Provider>>) {
    let config = account(host, port, user, password, Vec::new());
    let account_id = config.account_id().expect("the account id");
    let connections = ImapConnections::new();

    let Err(error) = connect_mail_providers(&connections, &config, &account_id).await else {
        panic!("the harness certificate was trusted without an accepted exception");
    };
    let AccountError::CertificateRejected { rejected, .. } = error else {
        panic!("the self-signed certificate was not offered as a rejection: {error}");
    };

    let accepted = CertificateException::new(rejected.server_name(), rejected.certificate());
    let config = account(host, port, user, password, vec![accepted]);
    let providers = connect_mail_providers(&connections, &config, &account_id)
        .await
        .expect("the dial with the accepted certificate");
    (account_id, providers)
}

/// The three subjects the harness seed pins; the list test's fixtures.
const NOTES: [&str; 3] = ["Harness note one", "Harness note two", "Harness note three"];

/// Counts the surface signals the app fires, so the live list test can assert the loop
/// republished the list a write flipped.
#[derive(Default)]
struct CountingObserver {
    /// How many surface changes have been signalled, across every surface.
    signals: AtomicUsize,
}

impl CountingObserver {
    /// The count so far.
    fn signals(&self) -> usize {
        self.signals.load(Ordering::SeqCst)
    }
}

impl AppObserver for CountingObserver {
    fn surface_changed(&self, _surface: Surface) {
        self.signals.fetch_add(1, Ordering::SeqCst);
    }
}

/// The INBOX's folder key as the pane projects it for `account`.
fn inbox_key(app: &App<Box<dyn Provider>>, account: &AccountId) -> String {
    app.mailbox_list()
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .and_then(|row| {
            row.folders
                .iter()
                .find(|folder| folder.role == Some(FolderRole::Inbox))
        })
        .map(|folder| folder.key.as_str().to_owned())
        .expect("the pane lists the account's INBOX")
}

/// The one list row whose subject matches, the list tests' fixture assertion.
fn row_by_subject<'a>(snapshot: &'a MailboxListSnapshot, subject: &str) -> &'a MessageRowView {
    snapshot
        .rows
        .iter()
        .find(|row| row.subject == subject)
        .unwrap_or_else(|| panic!("no row with subject {subject}"))
}

/// How many listed rows are unread.
fn unread_count(snapshot: &MailboxListSnapshot) -> usize {
    snapshot.rows.iter().filter(|row| row.unread).count()
}

/// Flips `subject`'s row to `read`, building the action's reference from the row the
/// list actually shows: the row's own key is the ref an action carries.
async fn mark_read(app: &App<Box<dyn Provider>>, subject: &str, read: bool) {
    let snapshot = app.mailbox_list();
    let RowKey::Message { account, key } = row_by_subject(&snapshot, subject).key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::MarkRead {
        row: RowRef::Message(MessageRef { account, key }),
        read,
    })
    .await
    .expect("the mark-read dispatch");
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
    let (account_id, providers) =
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
    let (account_id, providers) =
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
        mark_read(&app, subject, false).await;
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
    mark_read(&app, "Harness note two", true).await;
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
