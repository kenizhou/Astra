// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the mail sign-in's account half: the exchange's own tests
//! live in the exchange subfile, and this file owns the ordering the
//! module pins (the entry the rotation sink reads stands BEFORE the
//! dial, the persist reads the dial table back never the copy parsed
//! before the dial, a re-consent renews through the rotation path, and
//! every refusal path rolls the add back whole or restores a standing
//! account's entry and pool exactly as they stood). The rotation proof
//! runs over a rotating token endpoint, the only server shape that makes
//! a mid-dial rotation observable offline; every dial is a stand-in, and
//! the first sync waits on the runtime handle the wiring spawns on.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine_api::{CalendarWrites, Capabilities, MailboxWrites, Provider};
use engine_provider::ConnectionInfo;

use astra_account::{AccountConfig, AccountError, GraphTokenSource};

use crate::app_state::{AppPort, HostProvider};
use crate::commands_fakes::{scratch, FakeApp, FakeStore};
use crate::dial_table::AccountDials;
use crate::oauth::commands::RefusalKind;

use super::{complete_mail_signin, MailAccountWiring, MailGrantDial, PendingMailLogin};

#[path = "mail_add_tests/reconsent_tests.rs"]
mod reconsent_tests;

#[path = "mail_add_tests/exchange_tests.rs"]
mod exchange_tests;

#[path = "mail_add_tests/rotation_tests.rs"]
mod rotation_tests;

#[path = "mail_add_tests/sink_tests.rs"]
mod sink_tests;

/// A provider stand-in: one bound folder of a dialled mail account,
/// advertising mail and nothing else.
struct FakeFolder;

impl FakeFolder {
    /// One bound folder's stand-in.
    fn provider() -> HostProvider {
        Box::new(Self)
    }
}

#[async_trait]
impl Provider for FakeFolder {
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::new(Capabilities::none().with_mail())
    }
}

impl MailboxWrites for FakeFolder {}
impl CalendarWrites for FakeFolder {}

/// The pending handle a begun sign-in carries, pointed at the token
/// endpoint the test needs.
fn pending(token_endpoint: &str) -> PendingMailLogin {
    PendingMailLogin {
        email: "alice@example.test".to_owned(),
        imap_addr: "mail.example.test:993".to_owned(),
        server_name: "mail.example.test".to_owned(),
        security: astra_account::ConnectionSecurity::ImplicitTls,
        accepted: Some(
            astra_account::CertificateException::accepted(
                "mail.example.test",
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("the fixture fingerprint parses"),
        ),
        client_id: "client-xyz".to_owned(),
        client_secret: None,
        authorize_endpoint: "https://id.example.test/authorize".to_owned(),
        token_endpoint: token_endpoint.to_owned(),
        redirect_uri: "com.kenizhou.astra://oauth".to_owned(),
        scopes: vec!["offline_access".to_owned()],
        issuer: Some("https://id.example.test".to_owned()),
        state: "state-1".to_owned(),
        verifier: "verifier-1".to_owned(),
    }
}

/// The redirect the browser hands back for the pending's state, carrying
/// the `iss` the pending says the server advertises it sends.
fn callback() -> String {
    "com.kenizhou.astra://oauth?code=abc.def&state=state-1&iss=https%3A%2F%2Fid.example.test"
        .to_owned()
}

/// One scripted answer: the status line and the body.
type Answer = (&'static str, String);

/// Reads a whole small HTTP request, honouring `Content-Length`, so a
/// POST body split from its headers still arrives whole.
fn read_request(stream: &mut std::net::TcpStream) -> String {
    use std::io::Read;
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(read) = stream.read(&mut buf) {
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&buf[..read]);
        let text = String::from_utf8_lossy(&raw);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length: usize = head
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length: ")
                        .or_else(|| line.strip_prefix("content-length: "))
                })
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(0);
            if body.len() >= length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&raw).into_owned()
}

/// Writes one answer as a whole HTTP response.
fn respond(stream: &mut std::net::TcpStream, answer: Answer) {
    use std::io::Write;
    let (status, body) = answer;
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    );
    let _ = stream.write_all(response.as_bytes());
}

/// A token endpoint answering `answer` to every POST. Returns its URL.
fn token_endpoint(answer: fn() -> Answer) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = read_request(&mut stream);
            respond(&mut stream, answer());
        }
    });
    format!("http://{addr}/token")
}

/// A healthy exchange answer: a fresh pair with a refresh token.
fn healthy_answer() -> Answer {
    (
        "200 OK",
        r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT-1","refresh_token":"rt-9","scope":"offline_access"}"#
            .to_owned(),
    )
}

/// The exchange over a scripted endpoint, on a test's own runtime: the
/// document a completed sign-in answers, or the refusal it crossed.
fn exchange(pending: PendingMailLogin) -> Result<String, super::GraphAddFailure> {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let callback = callback();
    runtime.block_on(move || async move {
        super::complete_mail_login(
            &serde_json::to_string(&pending).expect("the pending handle serialises"),
            &callback,
            time::OffsetDateTime::now_utc(),
        )
        .await
    })
}

/// The document a healthy scripted exchange answers, the fixture every
/// account-half test starts from.
fn exchanged_document() -> String {
    exchange(pending(&token_endpoint(healthy_answer))).expect("the exchange answers a document")
}

/// What one scripted dial saw: the account id and the refresh token the
/// registered entry carried at dial time.
#[derive(Default)]
struct DialSaw {
    ids: Mutex<Vec<String>>,
    refresh_tokens: Mutex<Vec<String>>,
}

/// The dial seam answering the bound folders, recording what it saw.
fn ok_dial(saw: Arc<DialSaw>) -> MailGrantDial {
    dial_seam(saw, |_| async {
        Ok(vec![FakeFolder::provider(), FakeFolder::provider()])
    })
}

/// The dial seam over one scripted answer.
fn dial_seam<F, Fut>(saw: Arc<DialSaw>, answer: F) -> MailGrantDial
where
    F: Fn(Arc<GraphTokenSource>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Vec<HostProvider>, AccountError>> + Send + 'static,
{
    Arc::new(move |account, tokens, config, _pool| {
        let answer = answer(Arc::clone(&tokens));
        let account = account.as_str().to_owned();
        let refresh = config
            .imap
            .as_ref()
            .and_then(|imap| imap.oauth.as_ref())
            .map(|grant| grant.refresh_token.expose().to_owned())
            .unwrap_or_default();
        let saw = Arc::clone(&saw);
        Box::pin(async move {
            saw.ids.lock().unwrap().push(account);
            saw.refresh_tokens.lock().unwrap().push(refresh);
            answer.await
        })
    })
}

/// A dial seam that always fails with one verdict, built fresh per call
/// because the error does not copy.
fn failing_dial(saw: Arc<DialSaw>, err: fn() -> AccountError) -> MailGrantDial {
    dial_seam(saw, move |_| async move { Err(err()) })
}

/// The pieces over a fresh scratch index.
struct Host {
    app: Arc<FakeApp>,
    store: Arc<FakeStore>,
    dials: Arc<AccountDials>,
    refusals: Arc<crate::oauth::grant_refusals::GrantRefusals>,
    runtime: crate::runtime::AppRuntime,
}

impl Host {
    /// The pieces over a fresh scratch index.
    fn new(name: &str) -> Self {
        Self {
            app: FakeApp::new(),
            store: FakeStore::accepting(),
            dials: Arc::new(AccountDials::empty(&scratch(name))),
            refusals: Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
            runtime: crate::runtime::build_runtime().expect("the test runtime builds"),
        }
    }

    /// The completion's wiring over these pieces.
    fn wiring(&self) -> MailAccountWiring {
        MailAccountWiring {
            app: Arc::clone(&self.app) as Arc<dyn AppPort>,
            dials: Arc::clone(&self.dials),
            credentials: self.store.clone(),
            spawner: self.runtime.handle(),
            refusals: Arc::clone(&self.refusals),
        }
    }
}

/// The completion over the given seams, on the host's runtime.
fn complete(
    host: &Host,
    document: String,
    dial: MailGrantDial,
) -> Result<String, super::GraphAddFailure> {
    let wiring = host.wiring();
    host.runtime
        .block_on(move || complete_mail_signin(wiring, document, dial))
}

/// Waits until `probe` answers `Some`, with the bound every wait in this
/// file shares: anything slower is a fault, not a schedule.
fn wait_for<T>(probe: impl Fn() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = probe() {
            return value;
        }
        assert!(Instant::now() <= deadline, "the spawned half never landed");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The account id a grant document for `email` derives: the login and
/// the host together, lowercased.
fn mail_id(email: &str) -> String {
    format!("{}@mail.example.test", email.trim().to_lowercase())
}

/// The stored document's `[imap]` section, parsed back.
fn stored_imap(host: &Host, id: &str) -> astra_account::ImapAccount {
    let document = host
        .store
        .stored(id)
        .unwrap_or_else(|| panic!("the store holds the document for [{id}]"));
    astra_account::load_str(&document)
        .expect("the stored document loads")
        .imap
        .expect("the document is an imap one")
}

/// The documents the store holds for `id`, in the order they landed.
fn stored_documents(host: &Host, id: &str) -> Vec<String> {
    host.store
        .entries
        .lock()
        .unwrap()
        .iter()
        .filter(|(stored_id, _)| stored_id == id)
        .map(|(_, document)| document.clone())
        .collect()
}

#[test]
fn the_document_registers_before_the_dial_and_persists_after_it() {
    let host = Host::new("mail-add-ok");
    let saw = Arc::new(DialSaw::default());

    let address = complete(&host, exchanged_document(), ok_dial(Arc::clone(&saw)))
        .expect("the sign-in completes into an account");

    assert_eq!(address, "alice@example.test");
    let id = mail_id("alice@example.test");
    assert_eq!(
        saw.ids.lock().unwrap().as_slice(),
        [id.as_str()],
        "the dial ran for the derived id"
    );
    assert_eq!(
        saw.refresh_tokens.lock().unwrap().as_slice(),
        ["rt-9"],
        "the registered entry the dial read carries the fresh grant"
    );

    let imap = stored_imap(&host, &id);
    assert!(
        imap.password.is_none(),
        "an oauth account stores no long-lived password"
    );
    assert_eq!(
        imap.oauth.expect("the grant").refresh_token.expose(),
        "rt-9"
    );

    assert_eq!(host.dials.ids(), vec![id.clone()], "the account registers");
    let joined = wait_for(|| host.app.added.lock().unwrap().first().cloned());
    assert_eq!(joined, id, "the account joined the app");
    let synced = wait_for(|| host.app.synced.lock().unwrap().first().cloned());
    assert_eq!(synced, id, "the first sync runs in the background");
}

#[test]
fn a_refused_store_rolls_the_whole_add_back() {
    let host = Host {
        store: FakeStore::refusing(),
        ..Host::new("mail-add-store-refused")
    };

    let failure = complete(
        &host,
        exchanged_document(),
        ok_dial(Arc::new(DialSaw::default())),
    )
    .expect_err("the refused persist ends the add");

    assert_eq!(failure.refusal, RefusalKind::Indeterminate);
    assert!(
        host.dials.ids().is_empty(),
        "nothing stays registered for the refused add"
    );
    assert!(
        host.dials.indexed_ids().is_empty(),
        "the index names nothing the store does not hold"
    );
    assert!(
        host.store.stored(&mail_id("alice@example.test")).is_none(),
        "the store holds no orphaned grant"
    );
    assert!(
        host.app.added.lock().unwrap().is_empty(),
        "the account never joined the app"
    );
}

#[test]
fn a_dead_grant_during_the_dial_is_the_dead_word() {
    let host = Host::new("mail-add-dead-grant");

    let failure = complete(
        &host,
        exchanged_document(),
        failing_dial(Arc::new(DialSaw::default()), || {
            AccountError::SigninRejected("the refresh token was refused".to_owned())
        }),
    )
    .expect_err("the dead grant refuses the add");

    assert_eq!(failure.refusal, RefusalKind::Dead);
    assert!(
        failure.detail.contains("refresh token"),
        "the dial's own sentence rides in the detail: {}",
        failure.detail
    );
    assert!(
        host.dials.ids().is_empty(),
        "the pre-registered entry is taken back"
    );
    assert!(
        host.store.stored(&mail_id("alice@example.test")).is_none(),
        "no credential survives a dial that never answered"
    );
}

#[test]
fn a_failed_dial_rolls_a_new_add_back_whole() {
    let host = Host::new("mail-add-dial-failed");

    let failure = complete(
        &host,
        exchanged_document(),
        failing_dial(Arc::new(DialSaw::default()), || {
            AccountError::MailOauth("the connect never answered".to_owned())
        }),
    )
    .expect_err("the dial's failure is the completion's");

    assert_eq!(failure.refusal, RefusalKind::Indeterminate);
    assert!(
        host.dials.ids().is_empty() && host.dials.indexed_ids().is_empty(),
        "the pre-registered entry is taken back"
    );
    assert!(
        host.store.stored(&mail_id("alice@example.test")).is_none(),
        "no credential survives a dial that never answered"
    );
    assert!(host.app.added.lock().unwrap().is_empty());
}
