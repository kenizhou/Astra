// SPDX-License-Identifier: GPL-3.0-only
//! The harness-backed support the A3 live assist suite shares: the dial
//! through the product's own certificate acceptance walk and the
//! in-memory app over an engine the test holds beside it, the one-line
//! document a proof send carries, and the outbox-mediated delete sweep
//! that withdraws the extraction proof's own mail so the harness keeps
//! its seeded shape. The A1/A2 live suites' own harness modules are
//! private to them, so this file spells the same fixtures per suite:
//! duplication across gated suites, never across the product.

use std::sync::Arc;

use astra_account::{
    connect_mail_providers, AccountConfig, AccountError, CertificateException, ConnectionSecurity,
    ImapAccount, ImapConnections, Secret, SmtpAccount,
};
use astra_app::{Account, AiPass, App, AppObserver, Intent};
use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use engine_api::{AccountId, EmailAddress, Engine, MailEdit, Provider};

/// The subject the extraction proof's own send carries: unique among the
/// harness fixtures, so a row carrying it is this proof's mail and
/// nothing the seed delivered.
pub(super) const KNOWLEDGE_SUBJECT: &str = "Harness knowledge sample";

/// The words the extraction proof's send carries: durable knowledge about
/// the user (preferences, an ongoing project, a handling wish), written
/// first person from the account the extraction reads, so the batch the
/// pull collects always holds something worth landing whatever the seed's
/// own generic notes yield.
pub(super) const KNOWLEDGE_WORDS: &str = "A note to myself: I prefer short replies in plain \
                                          English. I am working on the Harbour launch with Ada; \
                                          the review lands next Friday. Please never share my \
                                          direct line.";

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

/// Builds an in-memory app over the dialled harness account and the
/// engine the caller holds beside it, the shape the desktop host
/// assembles with the AI bundle attached.
pub(super) fn assist_app(
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

/// The sweep's idempotency counter: the engine's outbox dedups edit ops
/// by (account, idempotency key) across every op state, so a key reused
/// within one engine answers the first run's settled op and the claim
/// refuses it as Settled. Every delete mints a fresh key.
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
            "a3-live-assist-cleanup:{}:{}",
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

/// The refresh intent spelled once, the sweep's bookend.
pub(super) async fn refresh(app: &App<Box<dyn Provider>>) {
    app.dispatch(Intent::RefreshMail)
        .await
        .expect("the refresh");
}
