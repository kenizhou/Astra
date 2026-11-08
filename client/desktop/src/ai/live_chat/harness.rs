// SPDX-License-Identifier: GPL-3.0-only
//! The harness-backed support the chat live proof runs on: the dial through
//! the product's certificate acceptance walk, the app over a FILE-BACKED
//! engine in the scratch data dir (the shape the spawned astra-cli children
//! read, because the CLI opens the data dir's own `astra.sqlite` and
//! `accounts.json`), the one-line document the seed send carries, and the
//! outbox-mediated delete sweep that withdraws the proof's seeded mail so
//! the harness keeps the shape the other suites pin. The sibling live
//! suites' harness modules are private to them, so this file spells the
//! same fixtures per suite: duplication across gated suites, never across
//! the product.

use std::sync::Arc;

use astra_account::{
    connect_mail_providers, AccountConfig, AccountError, CertificateException, ConnectionSecurity,
    ImapAccount, ImapConnections, Secret, SmtpAccount,
};
use astra_app::{Account, App, AppObserver, Intent};
use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use engine_api::{AccountId, EmailAddress, Engine, MailEdit, Provider};

use crate::agent::testkit::CollectingSink;
use crate::agent::{PendingApproval, SidecarRelay, StreamPayload};

/// The subject the proof's own seed send carries: unique among the harness
/// fixtures and the harness's own seeded notes, so a row carrying it is this
/// proof's mail and nothing else.
pub(super) const SEED_SUBJECT: &str = "Harbour lantern bloom note";

/// The words the seed send carries: the subject's own rare words ride the
/// body too, so the store's search index holds them whichever field the
/// model's query leans on.
pub(super) const SEED_WORDS: &str = "The lantern bloom over the harbour was counted at \
                                     forty two this season. This note exists so the \
                                     assistant's search has one distinguishable row to \
                                     find and act on.";

/// How long the turn driver waits for the done envelope: past the armed
/// budget's ceiling (the proof bounds it at 120 seconds), the watchdog's
/// grace, and the final text the model writes after the bite.
const DRIVE_BOUND: std::time::Duration = std::time::Duration::from_secs(300);

/// Drives one live turn the way the panel would: every approval card the
/// turn raises is answered through the relay's permission response, with
/// `approves` deciding each one, polling until the turn's own done
/// envelope crosses. A turn that never lands within the bound panics
/// loudly with the asks and events so far, so a hung run names its state.
pub(super) async fn drive_turn(
    relay: &SidecarRelay,
    sink: &CollectingSink,
    prompt_id: &str,
    approves: impl Fn(&PendingApproval) -> bool,
) {
    let deadline = std::time::Instant::now() + DRIVE_BOUND;
    let mut answered: Vec<String> = Vec::new();
    loop {
        let open: Vec<PendingApproval> = {
            let requests = sink.requests.lock().unwrap();
            requests
                .iter()
                .filter(|ask| !answered.contains(&ask.id))
                .cloned()
                .collect()
        };
        for ask in open {
            let approve = approves(&ask);
            relay
                .respond_permission(&ask.id, approve, false)
                .await
                .expect("the approval rides back");
            answered.push(ask.id);
        }
        let done = sink.agent_events.lock().unwrap().iter().any(
            |event| matches!(&event.event, StreamPayload::Done { prompt_id: id } if id == prompt_id),
        );
        if done {
            return;
        }
        if std::time::Instant::now() >= deadline {
            let asks = format!("{:?}", sink.requests.lock().unwrap());
            let seen = format!("{:?}", sink.agent_events.lock().unwrap());
            panic!("the live turn never landed done; asks: {asks}; events: {seen}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

/// A one-paragraph document carrying `text`, the smallest document a send
/// holds.
pub(super) fn document(text: &str) -> ComposerDocument {
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
            oauth: None,
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
        crypto: None,
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
    let Err(error) = connect_mail_providers(&connections, &config, &account_id, None).await else {
        panic!("the harness certificate was trusted without an accepted exception");
    };
    let AccountError::CertificateRejected { rejected, .. } = error else {
        panic!("the self-signed certificate was not offered as a rejection: {error}");
    };
    let accepted = CertificateException::new(rejected.server_name(), rejected.certificate());
    let config = harness_account(host, port, user, password, vec![accepted]);
    let providers = connect_mail_providers(&connections, &config, &account_id, None)
        .await
        .expect("the dial with the accepted certificate");
    (account_id, providers)
}

/// The quietest observer a live app needs: the suite reads state through
/// the app's own snapshots and pulls, so the signal path stays empty.
struct QuietObserver;

impl AppObserver for QuietObserver {
    fn surface_changed(&self, _surface: astra_app::Surface) {}
}

/// Builds the app over the dialled harness account and the engine the
/// caller holds (the test holds the file-backed one, the shape the
/// desktop host assembles).
pub(super) fn assist_app(
    engine: Arc<Engine>,
    account_id: AccountId,
    providers: Vec<Box<dyn Provider>>,
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
        .with_ai_pass(None),
    )
}

/// Writes the account index the spawned astra-cli children resolve account
/// ids against: the same file the desktop boot writes, one id, the harness
/// account's own.
pub(super) fn write_account_index(data_dir: &std::path::Path, account: &AccountId) {
    let index = serde_json::json!({ "accounts": [account.as_str()] });
    std::fs::write(
        data_dir.join("accounts.json"),
        serde_json::to_string_pretty(&index).expect("the index serialises"),
    )
    .expect("the account index writes");
}

/// The sweep's idempotency counter: the engine's outbox dedups edit ops by
/// (account, idempotency key) across every op state, so a key reused within
/// one engine answers the first run's settled op and the claim refuses it
/// as Settled. Every delete mints a fresh key.
static SWEEP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Deletes every message carrying one of `subjects` from `account` through
/// the engine's own outbox-mediated delete, over a provider a second dial
/// bound (the app keeps its providers private; the sweep needs one the
/// test holds). A message is addressed by its own key, so one provider can
/// withdraw any folder's copy, an INBOX delivery and its Sent filing
/// alike.
pub(super) async fn delete_subject_mail<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    subjects: &[&str],
) {
    let targets: Vec<_> = engine
        .messages(account)
        .await
        .expect("the account's stored messages")
        .into_iter()
        .filter(|message| {
            message
                .envelope
                .subject
                .as_deref()
                .is_some_and(|subject| subjects.contains(&subject))
        })
        .map(|message| message.id.key().clone())
        .collect();
    for target in targets {
        let idempotency = format!(
            "a2-live-chat-cleanup:{}:{}",
            std::process::id(),
            SWEEP.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        engine
            .edit_mail(
                provider,
                account,
                &idempotency,
                &MailEdit::Delete { target },
            )
            .await
            .expect("the cleanup delete");
    }
}

/// The refresh intent spelled once, the seed bookend and the sweep's own.
pub(super) async fn refresh(app: &App<Box<dyn Provider>>) {
    app.dispatch(Intent::RefreshMail)
        .await
        .expect("the refresh");
}

/// The sidecar checkout beside the desktop crate.
pub(super) fn sidecar_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sidecar")
}

/// The bundle the proof hosts: `dist/index.cjs` when present, else built by
/// running the sidecar's esbuild config through the resolved node.
pub(super) fn ensure_bundle(node: &std::path::Path) -> std::path::PathBuf {
    let bundle = sidecar_dir().join("dist").join("index.cjs");
    if bundle.is_file() {
        return bundle;
    }
    let output = std::process::Command::new(node)
        .arg("esbuild.config.mjs")
        .current_dir(sidecar_dir())
        .output()
        .expect("the bundle build spawns");
    assert!(
        output.status.success() && bundle.is_file(),
        "the sidecar bundle build failed (run npm --prefix sidecar ci once); stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    bundle
}

/// Whether the store's own search answers the seeded row before the model
/// runs: the preflight that makes a failed turn diagnosable (a store that
/// cannot find the seed cannot be searched by the model either).
pub(super) async fn seed_is_searchable(engine: &Engine, account: &AccountId) -> bool {
    let hits = engine
        .search_mail(account, "lantern bloom", 10)
        .await
        .expect("the store search runs");
    let keys = engine
        .messages(account)
        .await
        .expect("the account's stored messages")
        .into_iter()
        .filter(|message| message.envelope.subject.as_deref() == Some(SEED_SUBJECT))
        .map(|message| message.id.key().clone())
        .collect::<Vec<_>>();
    hits.hits.iter().any(|hit| keys.contains(&hit.key))
}
