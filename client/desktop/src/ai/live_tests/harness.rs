// SPDX-License-Identifier: GPL-3.0-only
//! The harness-backed half of the gated live AI suite: the tests that need
//! the local Stalwart harness beside the model connection. A real judge
//! call through the host's own implementation lands a verdict for the
//! genuinely seeded rich note (0003 and the ledger beside it), and the
//! incoming pass runs end to end over a real refresh with every stage row
//! under the pass's one correlation id. Both skip unless
//! `ASTRA_LIVE_HARNESS=1` stands beside the `ASTRA_LIVE_LLM` gate.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use astra_account::{
    connect_mail_providers, AccountConfig, AccountError, CertificateException, ConnectionSecurity,
    ImapAccount, ImapConnections, Secret, SmtpAccount,
};
use astra_ai::{CorrelationId, DlpPolicy, NewRule, RuleId};
use astra_app::{judge_cold, Account, App, AppObserver, FolderRef, Intent, MailContext};
use engine_api::{AccountId, EmailAddress, Engine, Provider};

use super::live;
use crate::ai::fixture;
use crate::ai::seams::{incoming_pass, LlmAiJudge};

/// The harness connection details from the environment, or `None` when the
/// harness gate is closed.
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

/// The harness SMTP submission port from the environment.
fn harness_smtp_port() -> String {
    std::env::var("ASTRA_HARNESS_SMTP_PORT").unwrap_or_else(|_| "2025".to_owned())
}

/// Dials the harness account through the certificate acceptance path a
/// person's first connect walks: refused, the rejection recorded, the one
/// certificate accepted, the re-dial bound.
async fn dial_harness(
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

/// Builds an in-memory app over the dialled harness account, the shape the
/// desktop host assembles with the AI bundle attached.
fn harness_app(
    account_id: AccountId,
    providers: Vec<Box<dyn Provider>>,
    ai: Option<astra_app::AiPass>,
) -> Arc<App<Box<dyn Provider>>> {
    let observer: Arc<dyn AppObserver> = Arc::new(QuietObserver);
    Arc::new(
        App::new(
            Arc::new(Engine::open_in_memory().expect("the engine store")),
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

/// The three subjects the harness seed pins; the pass test's fixtures.
const NOTES: [&str; 3] = ["Harness note one", "Harness note two", "Harness note three"];

/// The provider keys of the three seeded notes, read back out of the
/// INBOX list a selection published: the unified list carries no rows in
/// this slice (rows are a folder-scope field), so the test selects the
/// INBOX first, exactly the shape the list's own live suite drives.
fn note_keys(app: &App<Box<dyn Provider>>) -> Vec<(AccountId, engine_api::ProviderKey)> {
    let snapshot = app.mailbox_list();
    NOTES
        .iter()
        .map(|subject| {
            let row = snapshot
                .rows
                .iter()
                .find(|row| row.subject == *subject)
                .unwrap_or_else(|| panic!("no row with subject {subject}"));
            let astra_viewmodel::RowKey::Message { account, key } = row.key.clone() else {
                panic!("a flat list row is keyed by its message");
            };
            (account, key)
        })
        .collect()
}

/// Selects the account's INBOX, the folder row with the Inbox role.
async fn select_inbox(app: &App<Box<dyn Provider>>, account: &AccountId) {
    let snapshot = app.mailbox_list();
    let key = snapshot
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .and_then(|row| {
            row.folders
                .iter()
                .find(|folder| folder.role == Some(astra_viewmodel::FolderRole::Inbox))
        })
        .map(|folder| folder.key.as_str().to_owned())
        .expect("the pane lists the account's INBOX");
    let folder = FolderRef::from_parts(account.as_str(), key).expect("the folder reference");
    app.dispatch(Intent::SelectFolder { folder })
        .await
        .expect("the INBOX selection");
}

/// Flips `subject`'s row to `read`, building the action's reference from
/// the row the selected folder's list actually shows: the product's own
/// write path, the same arm the mail live suite drives.
async fn mark_note(app: &App<Box<dyn Provider>>, subject: &str, read: bool) {
    let snapshot = app.mailbox_list();
    let row = snapshot
        .rows
        .iter()
        .find(|row| row.subject == subject)
        .unwrap_or_else(|| panic!("no row with subject {subject}"));
    let astra_viewmodel::RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::MarkRead {
        row: astra_app::RowRef::Message(astra_app::MessageRef { account, key }),
        read,
    })
    .await
    .expect("the mark-read dispatch");
}

/// Selects the account's Projects folder, whose row the pane lists by name
/// (Projects carries no standard role).
async fn select_projects(app: &App<Box<dyn Provider>>, account: &AccountId) {
    let snapshot = app.mailbox_list();
    let key = snapshot
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .and_then(|row| row.folders.iter().find(|folder| folder.name == "Projects"))
        .map(|folder| folder.key.as_str().to_owned())
        .expect("the pane lists the account's Projects folder");
    let folder = FolderRef::from_parts(account.as_str(), key).expect("the folder reference");
    app.dispatch(Intent::SelectFolder { folder })
        .await
        .expect("the Projects selection");
}

/// The cold judge live on the seeded rich note: a real judge call through
/// the host's own judge implementation over a genuinely seeded harness
/// message, the verdict recorded in 0003 and the judged attempt in the
/// ledger. The note is opened to read its body (which marks it read) and
/// put back to unread afterwards, so the harness keeps its seeded shape.
#[test]
fn judge_cold_live_on_the_seeded_note_records_verdict_and_rows() {
    let Some((client, ledger)) = live("live-judge-cold") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live ai harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let seam = LlmAiJudge::new(client);
    let correlation = CorrelationId::new("corr-live-judge-cold".to_string());

    fixture::runtime().block_on(|| async {
        let (account_id, providers) = dial_harness(&host, &port, &user, &password).await;
        let app = harness_app(account_id.clone(), providers, None);
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the refresh");
        select_projects(&app, &account_id).await;

        // The rich note's body is html inside multipart/related (its plain
        // part is an attachment), so the context takes the sanitised html
        // the reading pipeline produced: the note as the product reads it.
        let snapshot = app.mailbox_list();
        let row = snapshot
            .rows
            .iter()
            .find(|row| row.subject == "Harness rich note")
            .unwrap_or_else(|| panic!("the seeded rich note is listed"));
        let astra_viewmodel::RowKey::Message { account, key } = row.key.clone() else {
            panic!("a flat list row is keyed by its message");
        };
        let message = astra_app::MessageRef {
            account: account.clone(),
            key: key.clone(),
        };
        app.dispatch(Intent::OpenMessage {
            message: message.clone(),
        })
        .await
        .expect("the open");
        let reading = app.reading().expect("the open published a body");
        let body = reading
            .plain
            .or(reading.html)
            .expect("the rich note carries a readable body");

        let mail = MailContext {
            account: account.clone(),
            key: key.clone(),
            from_name: reading.from_name.clone(),
            from_address: reading.from_address.clone(),
            subject: reading.subject.clone(),
            body,
        };
        judge_cold(&mail, &correlation, &seam, &ledger.store()).await;

        // Put the note back the way the seed left it.
        app.dispatch(Intent::MarkRead {
            row: astra_app::RowRef::Message(message),
            read: false,
        })
        .await
        .expect("the unread reset");

        let rows = ledger
            .store()
            .ai_results_for_message(account.as_str(), key.as_str())
            .expect("the results read");
        assert!(
            rows.iter().any(|row| row.stage == "cold"
                && ["yes", "no", "abstain_default"].contains(&row.verdict.as_str())
                && row.correlation.as_str() == "corr-live-judge-cold"),
            "the cold verdict landed its 0003 row under the call's correlation: {rows:?}"
        );
    });

    let rows = ledger.rows();
    assert!(
        rows.iter()
            .any(|row| row.correlation == "corr-live-judge-cold"
                && row.outcome.starts_with("judge:")
                && row.tier == "nano"),
        "the judged attempt landed its ledger row with its verdict: {rows:?}"
    );
}

/// The incoming pass end to end: a real refresh arms the pass, the driver
/// the host runs judges the seeded unread mail through the real seams, and
/// every stage row lands in 0003 under the pass's one correlation id,
/// beside the pass's ledger rows. The stored rule's condition matches
/// nothing the seed delivers, so the pass judges without executing any
/// write: the harness keeps its seeded shape.
#[test]
fn the_incoming_pass_runs_over_a_real_refresh_live() {
    let Some((client, ledger)) = live("live-pass") else {
        eprintln!("skipping live ai test: ASTRA_LIVE_LLM is not 1");
        return;
    };
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live ai harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let store = ledger.store();
    let rule = NewRule {
        id: RuleId::generate(),
        name: "Synthesizers".to_owned(),
        condition: "The message discusses vintage synthesizers.".to_owned(),
        actions: r#"["mark_read"]"#.to_owned(),
        enabled: true,
    };
    store.create_rule(&rule).expect("the rule stores");
    let ai = incoming_pass(
        Some(Arc::clone(&client)),
        Arc::clone(&store),
        Ok(DlpPolicy::Allow),
        None,
    )
    .expect("the pass arms over the live client");

    fixture::runtime().block_on(|| async {
        let (account_id, providers) = dial_harness(&host, &port, &user, &password).await;
        let app = harness_app(account_id.clone(), providers, Some(ai));
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the first refresh");
        select_inbox(&app, &account_id).await;
        // A warm volume holds the read state an earlier suite left (the mail
        // live battery pins note two read), and the pass judges unread mail
        // only: put every note back to unread through the product's own
        // write path and refresh again, so the one pass the driver then
        // serves judges the whole reset state.
        for subject in NOTES {
            mark_note(&app, subject, false).await;
        }
        app.dispatch(Intent::RefreshMail)
            .await
            .expect("the reset refresh");
        // Spawned after the arming refresh: the stored notify permit runs
        // exactly one pass over the reset state, one correlation for every
        // row the assertions read.
        let driver = tokio::spawn({
            let app = Arc::clone(&app);
            async move { app.drive_incoming_pass().await }
        });

        let keys = note_keys(&app);
        assert_eq!(keys.len(), NOTES.len(), "the three seeded notes are listed");
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            let judged: Vec<Vec<astra_ai::AiResultRow>> = keys
                .iter()
                .map(|(account, key)| {
                    store
                        .ai_results_for_message(account.as_str(), key.as_str())
                        .expect("the results read")
                })
                .collect();
            let stages_done = |rows: &Vec<astra_ai::AiResultRow>| {
                rows.iter().any(|row| row.stage == "rule_match")
                    && rows.iter().any(|row| row.stage == "categorise")
                    && rows.iter().any(|row| row.stage == "cold")
            };
            if judged.iter().all(stages_done) {
                let mut correlations: Vec<_> = judged
                    .iter()
                    .flat_map(|rows| rows.iter().map(|row| row.correlation.as_str().to_owned()))
                    .collect();
                correlations.sort();
                correlations.dedup();
                assert_eq!(
                    correlations.len(),
                    1,
                    "every stage row of the pass shares one correlation: {correlations:?}"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the pass never judged the three notes within five minutes"
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        driver.abort();
    });

    let rows = ledger.rows();
    let model_calls = rows
        .iter()
        .filter(|row| row.outcome == "ok" || row.outcome.starts_with("judge:"))
        .count();
    assert!(
        model_calls >= 3,
        "the pass's model attempts landed ledger rows: {rows:?}"
    );
}
