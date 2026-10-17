// SPDX-License-Identifier: GPL-3.0-only
//! The harness-backed support the A4 live documents suite shares: the
//! dial through the product's own certificate acceptance walk, the
//! in-memory app over an engine the test holds beside it, the folder
//! selection and note fixtures the A1 pass suite spells, and the wait
//! for the incoming pass's decided categorisation over the seeded notes
//! (the rollup source and the split derivation both ride those rows).
//! The earlier suites' harness modules are private to them, so this
//! file spells the same fixtures per suite: duplication across gated
//! suites, never across the product.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use astra_account::{
    connect_mail_providers, AccountConfig, AccountError, CertificateException, ConnectionSecurity,
    ImapAccount, ImapConnections, Secret, SmtpAccount,
};
use astra_app::{Account, AiPass, App, AppObserver, FolderRef, Intent, MessageRef};
use astra_viewmodel::{FolderRole, RowKey};
use engine_api::{AccountId, EmailAddress, Engine, Provider};

/// The three subjects the harness seed pins; the documents suite's mail.
pub(super) const NOTES: [&str; 3] = ["Harness note one", "Harness note two", "Harness note three"];

/// The harness connection details from the environment, or `None` when the
/// harness gate is closed.
pub(super) fn harness() -> Option<(String, String, String, String)> {
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

/// The harness SMTP submission port from the environment.
fn harness_smtp_port() -> String {
    std::env::var("ASTRA_HARNESS_SMTP_PORT").unwrap_or_else(|_| "2025".to_owned())
}

/// The harness account config, both legs over the one self-signed server,
/// with the certificate exceptions its owner has accepted.
fn harness_account(
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
        shape: astra_account::AccountShape::default(),
    }
}

/// Dials the harness account through the certificate acceptance path a
/// person's first connect walks: refused, the rejection recorded, the one
/// certificate accepted, the re-dial bound.
pub(super) async fn dial(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (AccountId, Vec<Box<dyn Provider>>) {
    let config = harness_account(host, port, user, password, Vec::new());
    let account_id = config.account_id().expect("the account id");
    let connections = ImapConnections::new();
    let Err(error) = connect_mail_providers(&connections, &config, &account_id).await else {
        panic!("the harness certificate was trusted without an accepted exception");
    };
    let AccountError::CertificateRejected { rejected, .. } = error else {
        panic!("the self-signed certificate was not offered as a rejection: {error}");
    };
    let accepted = CertificateException::new(rejected.server_name(), rejected.certificate());
    let config = harness_account(host, port, user, password, vec![accepted]);
    let providers = connect_mail_providers(&connections, &config, &account_id)
        .await
        .expect("the dial with the accepted certificate");
    (account_id, providers)
}

/// The quietest observer a live app needs: the suite reads state through
/// the app's own snapshots, so the signal path stays empty.
struct QuietObserver;

impl AppObserver for QuietObserver {
    fn surface_changed(&self, _surface: astra_app::Surface) {}
}

/// Builds an in-memory app over the dialled harness account and the engine
/// the caller holds beside it, the shape the desktop host assembles with
/// the AI bundle attached.
pub(super) fn docs_app(
    engine: Arc<Engine>,
    account_id: AccountId,
    providers: Vec<Box<dyn Provider>>,
    ai: Option<AiPass>,
) -> Arc<App<Box<dyn Provider>>> {
    let observer: Arc<dyn AppObserver> = Arc::new(QuietObserver);
    Arc::new(
        App::new(
            engine,
            vec![Account {
                id: account_id,
                providers,
                identity: EmailAddress::new("astra1@example.test"),
            }],
            None,
            observer,
        )
        .with_ai_pass(ai),
    )
}

/// Selects the account's INBOX, the folder row with the Inbox role.
pub(super) async fn select_inbox(app: &App<Box<dyn Provider>>, account: &AccountId) {
    let snapshot = app.mailbox_list();
    let key = snapshot
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .and_then(|row| {
            row.folders
                .iter()
                .find(|folder| folder.role == Some(FolderRole::Inbox))
        })
        .map(|folder| folder.key.as_str().to_owned())
        .expect("the pane lists the account's INBOX");
    let folder = FolderRef::from_parts(account.as_str(), key).expect("the folder reference");
    app.dispatch(Intent::SelectFolder { folder })
        .await
        .expect("the INBOX selection");
}

/// The selected folder's row carrying `subject`, as the message reference
/// the intents address.
fn row_by_subject(app: &App<Box<dyn Provider>>, subject: &str) -> MessageRef {
    let snapshot = app.mailbox_list();
    let row = snapshot
        .rows
        .iter()
        .find(|row| row.subject == subject)
        .unwrap_or_else(|| panic!("no row with subject {subject}"));
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    MessageRef { account, key }
}

/// The seeded notes' references, read out of the selected INBOX's list.
pub(super) fn note_refs(app: &App<Box<dyn Provider>>) -> Vec<MessageRef> {
    NOTES
        .iter()
        .map(|subject| row_by_subject(app, subject))
        .collect()
}

/// Puts every seeded note back to unread through the product's own write
/// path, so the pass the driver then serves judges the whole reset state
/// (the pass judges unread mail only).
pub(super) async fn reset_notes_unread(app: &App<Box<dyn Provider>>) {
    for subject in NOTES {
        let message = row_by_subject(app, subject);
        app.dispatch(Intent::MarkRead {
            row: astra_app::RowRef::Message(message),
            read: false,
        })
        .await
        .expect("the unread reset");
    }
}

/// One note's civil UTC tuple as the generation window's filter compares
/// it, `None` when the stored copy carries no date at all.
pub(super) fn civil_of(message: &engine_api::Message) -> Option<(i32, u8, u8, u8, u8, u8)> {
    let date = message.received_at?;
    Some((
        date.year(),
        date.month(),
        date.day(),
        date.hour(),
        date.minute(),
        date.second(),
    ))
}

/// The unix second as the same civil tuple, `None` outside the calendar.
pub(super) fn civil_unix(unix: i64) -> Option<(i32, u8, u8, u8, u8, u8)> {
    let instant = time::OffsetDateTime::from_unix_timestamp(unix).ok()?;
    Some((
        instant.year(),
        u8::from(instant.month()),
        instant.day(),
        instant.hour(),
        instant.minute(),
        instant.second(),
    ))
}

/// Waits until the incoming pass has landed a decided categorisation row
/// for every note, the rollup source the documents ride: a note whose only
/// categorise row was a skip holds no category, so the wait is for the
/// decided shape. Five minutes, the A1 suite's own bound.
pub(super) async fn wait_categorised(
    store: &astra_ai::AiStore,
    account: &AccountId,
    notes: &[MessageRef],
) {
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let decided = notes.iter().all(|note| {
            store
                .ai_results_for_message(account.as_str(), note.key.as_str())
                .expect("the results read")
                .iter()
                .any(|row| row.stage == "categorise" && row.category.is_some())
        });
        if decided {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the pass never categorised the three notes within five minutes"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
