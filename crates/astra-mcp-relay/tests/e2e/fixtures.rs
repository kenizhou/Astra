// SPDX-License-Identifier: GPL-3.0-only
//! The fixtures the gated e2e proofs share: the harness dial restated from
//! the live suites (astra-app's own helpers are crate-private test code),
//! the two-account app over it, the server with its gate, the pipe and
//! relay plumbing, and the stdio client whose loop the proofs speak
//! through. The proofs themselves live in `main.rs`.

use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use astra_account::{
    connect_mail_providers, AccountConfig, AccountError, AccountShape, CertificateException,
    ConnectionSecurity, ImapAccount, ImapConnections, Secret, SmtpAccount,
};
use astra_app::{Account, App, AppObserver, Intent, MessageRef, RowRef, Surface};
use astra_mcp::backend::MailBackend;
use astra_mcp::ledger::ToolLedger;
use astra_mcp::server::Server;
use astra_mcp::transport::{Endpoint, Listener};
use astra_viewmodel::{FolderRole, MailboxListSnapshot, MessageRowView, RowKey};
use engine_api::{AccountId, EmailAddress, Engine, Provider};

/// The built relay binary, as cargo hands its path to the test.
pub(crate) const BIN: &str = env!("CARGO_BIN_EXE_astra-mcp-relay");

/// The bound on one request's round trip. Generous beside the relay's own
/// suite, because a tools/call here fetches real mail over real IMAP inside
/// the call; a wedged stack must still fail the test, not hang it.
pub(crate) const BOUND: Duration = Duration::from_secs(30);

/// Long enough for the relay to be parked reading the pipe when the next
/// write comes: a real client opens with a notification that draws no
/// reply, and the request after it must still go through.
pub(crate) const SETTLE: Duration = Duration::from_millis(250);

/// The receiving account's address, the second harness user.
pub(crate) const RECIPIENT: &str = "astra2@example.test";

/// The seeded corpus the search pin expects, in the pinned newest-first
/// order: the rich note in Projects, then the three INBOX notes. The
/// drafts suite keeps a further harness-titled draft on the volume by
/// design, so a search's assertion filters to these four rather than the
/// whole answer.
pub(crate) const SEEDED: [&str; 4] = [
    "Harness rich note",
    "Harness note three",
    "Harness note two",
    "Harness note one",
];

/// The harness connection details from the environment, or `None` to skip.
pub(crate) fn harness() -> Option<(String, String, String, String)> {
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

/// The harness SMTP submission port: the config's submission leg names the
/// same server the IMAP leg dials.
pub(crate) fn harness_smtp_port() -> String {
    std::env::var("ASTRA_HARNESS_SMTP_PORT").unwrap_or_else(|_| "2025".to_owned())
}

/// The harness account config, with the certificate exceptions its owner
/// has accepted (none for the first dial). Both legs name the one server,
/// so one accepted certificate covers them both.
pub(crate) fn account(
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
        shape: AccountShape::default(),
    }
}

/// Dials one harness account through the certificate acceptance path a
/// person's first connect walks: refused first, the rejection recorded, the
/// one certificate accepted for the one server, then the dial returns the
/// account id and every provider it bound.
pub(crate) async fn dial_with_accepted_certificate(
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

/// The observer the app is built over: present, because the app requires
/// one, and silent, because these proofs assert through snapshots rather
/// than signal counts.
struct Silent;

impl AppObserver for Silent {
    fn surface_changed(&self, _surface: Surface) {}
}

/// The two-account app over one in-memory store (astra1 first, astra2
/// second): the shape the delivery proofs build, so a real configured
/// account exists beside the exposed one for the gate to refuse.
pub(crate) async fn both_accounts_app(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (Arc<App<Box<dyn Provider>>>, AccountId, AccountId) {
    let (first, first_providers) = dial_with_accepted_certificate(host, port, user, password).await;
    let (second, second_providers) =
        dial_with_accepted_certificate(host, port, RECIPIENT, password).await;
    let observer: Arc<dyn AppObserver> = Arc::new(Silent);
    let app = Arc::new(App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![
            Account {
                id: first.clone(),
                providers: first_providers,
                identity: EmailAddress::new("astra1@example.test"),
            },
            Account {
                id: second.clone(),
                providers: second_providers,
                identity: EmailAddress::new(RECIPIENT),
            },
        ],
        None,
        observer,
    ));
    (app, first, second)
}

/// The server over the real app and an in-memory ledger, exposing exactly
/// `allowed`. The caller keeps its own handle on the app, so snapshot
/// assertions ride the product's own state beside the wire.
pub(crate) fn server_over(app: &Arc<App<Box<dyn Provider>>>, allowed: &[AccountId]) -> Arc<Server> {
    let server = Arc::new(Server::new(
        Arc::clone(app) as Arc<dyn MailBackend>,
        Arc::new(astra_ai::AiStore::open_in_memory().expect("the ledger opens"))
            as Arc<dyn ToolLedger>,
    ));
    server
        .gate()
        .set_allowed(allowed.iter().map(|id| id.as_str().to_owned()));
    server
}

/// A pipe name unique to one test, so no sibling test's endpoint answers.
pub(crate) fn pipe_name(name: &str) -> String {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    format!(r"\\.\pipe\astra-mcp-e2e-{name}-{unique}")
}

/// Binds `endpoint` and serves `server` on it in the background.
pub(crate) fn serve_on(endpoint: &str, server: Arc<Server>) {
    let listener = Listener::bind(&Endpoint::parse(endpoint).expect("the endpoint parses"))
        .expect("the listener binds");
    tokio::spawn(async move {
        listener.accept(server).await;
    });
}

/// A stdio MCP client over the spawned relay: one request writer and one
/// bounded response reader, the shape a real client's loop takes.
pub(crate) struct WireClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl WireClient {
    /// Spawns the relay against `endpoint` with piped stdio and no chance
    /// of outliving a failing test.
    pub(crate) fn spawn(endpoint: &str) -> Self {
        let mut child = Command::new(BIN)
            .arg("--endpoint")
            .arg(endpoint)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("the relay binary runs");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Self {
            child,
            stdin: Some(stdin),
            stdout,
        }
    }

    /// Sends one request and reads its one answer, bounded.
    pub(crate) async fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        let mut frame =
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        frame.push('\n');
        let stdin = self.stdin.as_mut().expect("the client is still open");
        stdin
            .write_all(frame.as_bytes())
            .await
            .expect("the request writes");
        stdin.flush().await.expect("the request flushes");
        let mut line = Vec::new();
        tokio::time::timeout(BOUND, self.stdout.read_until(b'\n', &mut line))
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for the {method} answer"))
            .expect("the answer reads");
        assert_eq!(line.pop(), Some(b'\n'), "the frame ends with the newline");
        let answered: Value =
            serde_json::from_slice(&line).expect("the server answers one JSON value");
        assert_eq!(
            answered["id"],
            json!(id),
            "the answer carries its id: {answered}"
        );
        answered
    }

    /// Sends the notification a real client opens with. It draws no reply;
    /// the settle leaves the relay parked on a silent pipe, which the next
    /// request must still cross.
    pub(crate) async fn initialized(&mut self) {
        let stdin = self.stdin.as_mut().expect("the client is still open");
        stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .await
            .expect("the notification writes");
        stdin.flush().await.expect("the notification flushes");
        tokio::time::sleep(SETTLE).await;
    }

    /// Closes stdin and waits for the relay's clean exit.
    pub(crate) async fn finish(&mut self) {
        drop(self.stdin.take());
        let status = tokio::time::timeout(BOUND, self.child.wait())
            .await
            .expect("the relay exits")
            .expect("exit status");
        assert!(status.success(), "the relay ends cleanly: {status}");
    }
}

/// The INBOX's folder key as the pane projects it for `account`.
pub(crate) fn inbox_key(app: &App<Box<dyn Provider>>, account: &AccountId) -> String {
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

/// The one list row whose subject matches, the snapshot assertions' fixture.
pub(crate) fn row_by_subject<'a>(
    snapshot: &'a MailboxListSnapshot,
    subject: &str,
) -> &'a MessageRowView {
    snapshot
        .rows
        .iter()
        .find(|row| row.subject == subject)
        .unwrap_or_else(|| panic!("no row with subject {subject}"))
}

/// Flips `subject`'s row to `read` through the product's own dispatch: the
/// reset that puts the corpus back to a known state.
pub(crate) async fn mark_read(app: &App<Box<dyn Provider>>, subject: &str, read: bool) {
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

/// The subjects of a listing's or search's rows, in order.
pub(crate) fn subjects(structured: &Value) -> Vec<&str> {
    structured["rows"]
        .as_array()
        .expect("the rows answer a list")
        .iter()
        .map(|row| {
            row["subject"]
                .as_str()
                .expect("every row names its subject")
        })
        .collect()
}

/// One account-scoped tool call for `account`, through the wire.
pub(crate) async fn call_for_account(
    client: &mut WireClient,
    id: u64,
    tool: &str,
    account: &str,
) -> Value {
    client
        .request(
            id,
            "tools/call",
            json!({"name": tool, "arguments": {"account": account}}),
        )
        .await
}
