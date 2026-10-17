// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the JMAP sign-in's account half: the document a completed
//! discovery exchange answers becomes a registered, dialled, persisted
//! account joined to the app; the ordering this module owns (the entry
//! the rotation sink reads stands BEFORE the dial, and the persist reads
//! the dial table back, never the copy parsed before the dial) is proven
//! over a rotating token endpoint, the only server shape that makes a
//! mid-dial rotation observable; and every refusal path rolls the add
//! back whole, keeps a standing account's entry as it stood, and goes
//! through the rotation path for a re-consent. Every dial is a stand-in:
//! no test touches a JMAP server, and the first sync waits on the runtime
//! handle the wiring spawns on.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine_api::{AccountId, CalendarWrites, Capabilities, MailboxWrites, Provider};
use engine_provider::ConnectionInfo;

use astra_account::{AccountConfig, AccountError, GraphTokenSource};

use crate::app_state::{AppPort, HostProvider};
use crate::commands_fakes::{scratch, FakeApp, FakeStore};
use crate::dial_table::AccountDials;
use crate::oauth::commands::RefusalKind;

use super::{complete_jmap_signin, GraphAddFailure, JmapAccountWiring, JmapDial};

#[path = "jmap_add_tests/sink_tests.rs"]
mod sink_tests;

#[path = "jmap_add_tests/rotation_tests.rs"]
mod rotation_tests;

/// A provider stand-in: the one account-wide JMAP provider, advertising
/// mail and nothing else, the shape the scripted session serves.
struct FakeJmap;

impl FakeJmap {
    /// The account-wide provider's stand-in.
    fn provider() -> HostProvider {
        Box::new(Self)
    }
}

#[async_trait]
impl Provider for FakeJmap {
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::new(Capabilities::none().with_mail())
    }
}

impl MailboxWrites for FakeJmap {}
impl CalendarWrites for FakeJmap {}

/// The document a completed sign-in answers: the `[jmap]` section with
/// the grant and no password, its token endpoint pointed wherever the
/// test needs (the rotating endpoint for the ordering proof, an unrouted
/// host everywhere else).
fn grant_document(email: &str, token_endpoint: &str) -> String {
    format!(
        concat!(
            "[jmap]\n",
            "email = \"{email}\"\n",
            "base_url = \"https://mail.example.test\"\n",
            "\n[jmap.oauth]\n",
            "client_id = \"client-xyz\"\n",
            "refresh_token = \"rt-1\"\n",
            "authorize_endpoint = \"https://mail.example.test/oauth/authorize\"\n",
            "token_endpoint = \"{token_endpoint}\"\n",
            "redirect_uri = \"com.kenizhou.astra://oauth\"\n",
            "scopes = [\"offline_access\"]\n",
        ),
        email = email,
        token_endpoint = token_endpoint,
    )
}

/// The unrouted token endpoint every stand-in dial test names: no
/// request can leave the device, and none needs to.
const QUIET_TOKEN_ENDPOINT: &str = "https://auth.example.test/token";

/// What one scripted dial saw: the account id and the refresh token the
/// registered entry carried at dial time.
#[derive(Default)]
struct DialSaw {
    ids: Mutex<Vec<String>>,
    refresh_tokens: Mutex<Vec<String>>,
}

/// The dial seam answering the one account-wide provider, recording what
/// it saw.
fn ok_dial(saw: Arc<DialSaw>) -> JmapDial {
    dial_seam(saw, |_| async { Ok(FakeJmap::provider()) })
}

/// The dial seam over one scripted answer.
fn dial_seam<F, Fut>(saw: Arc<DialSaw>, answer: F) -> JmapDial
where
    F: Fn(Arc<GraphTokenSource>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<HostProvider, AccountError>> + Send + 'static,
{
    Arc::new(move |account, tokens, config| {
        let answer = answer(Arc::clone(&tokens));
        let account = account.as_str().to_owned();
        let refresh = config
            .jmap
            .as_ref()
            .and_then(|jmap| jmap.oauth.as_ref())
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

/// A dial seam that always fails with one verdict, built fresh per
/// call because the error does not copy.
fn failing_dial(saw: Arc<DialSaw>, err: fn() -> AccountError) -> JmapDial {
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
    fn wiring(&self) -> JmapAccountWiring {
        JmapAccountWiring {
            app: Arc::clone(&self.app) as Arc<dyn AppPort>,
            dials: Arc::clone(&self.dials),
            credentials: self.store.clone(),
            spawner: self.runtime.handle(),
            refusals: Arc::clone(&self.refusals),
        }
    }
}

/// The completion over the given seams, on the host's runtime.
fn complete(host: &Host, document: String, dial: JmapDial) -> Result<String, GraphAddFailure> {
    let wiring = host.wiring();
    host.runtime
        .block_on(move || complete_jmap_signin(wiring, document, dial))
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

/// The account id a grant document for `email` derives.
fn jmap_id(email: &str) -> String {
    format!("{}@jmap:mail.example.test", email.trim().to_lowercase())
}

/// The stored document's `[jmap]` section, parsed back.
fn stored_jmap(host: &Host, id: &str) -> astra_account::JmapAccount {
    let document = host
        .store
        .stored(id)
        .unwrap_or_else(|| panic!("the store holds the document for [{id}]"));
    astra_account::load_str(&document)
        .expect("the stored document loads")
        .jmap
        .expect("the document is a jmap one")
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

/// A standing jmap account's stored shape: a pinned id, a dav section,
/// and an accepted certificate the grant does not own.
fn standing_config() -> AccountConfig {
    astra_account::load_str(concat!(
        "id = \"alice@example.test@jmap:mail.example.test\"\n",
        "[jmap]\n",
        "email = \"Alice@Example.test\"\n",
        "base_url = \"https://mail.example.test\"\n",
        "\n[jmap.oauth]\n",
        "client_id = \"client-old\"\n",
        "refresh_token = \"rt-old\"\n",
        "authorize_endpoint = \"https://mail.example.test/oauth/authorize\"\n",
        "token_endpoint = \"https://auth.example.test/token\"\n",
        "redirect_uri = \"com.kenizhou.astra://oauth\"\n",
        "scopes = [\"offline_access\"]\n",
        "\n[dav]\n",
        "caldav_url = \"https://dav.example.test\"\n",
        "username = \"Alice@Example.test\"\n",
        "password = \"dav-secret\"\n",
        "\n[[certificate_exception]]\n",
        "server_name = \"mail.example.test\"\n",
        "sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n",
    ))
    .expect("the standing fixture loads")
}

/// Registers a standing account under `email`'s derived id.
fn register_stored(host: &Host, config: AccountConfig, email: &str) {
    let id = AccountId::try_from(jmap_id(email).as_str()).unwrap();
    host.dials
        .register(&id, config, astra_account::ImapConnections::new())
        .expect("the standing account registers");
}

#[test]
fn the_document_registers_before_the_dial_and_persists_after_it() {
    let host = Host::new("jmap-add-ok");
    let saw = Arc::new(DialSaw::default());

    let address = complete(
        &host,
        grant_document("Alice@Example.test", QUIET_TOKEN_ENDPOINT),
        ok_dial(Arc::clone(&saw)),
    )
    .expect("the sign-in completes into an account");

    assert_eq!(address, "Alice@Example.test");
    let id = jmap_id("Alice@Example.test");
    assert_eq!(
        saw.ids.lock().unwrap().as_slice(),
        [id.as_str()],
        "the dial ran for the derived id"
    );
    assert_eq!(
        saw.refresh_tokens.lock().unwrap().as_slice(),
        ["rt-1"],
        "the registered entry the dial read carries the fresh grant"
    );

    let jmap = stored_jmap(&host, &id);
    assert_eq!(jmap.email, "Alice@Example.test");
    assert_eq!(jmap.base_url, "https://mail.example.test");
    assert!(
        jmap.password.is_none(),
        "an oauth account stores no long-lived password"
    );
    let grant = jmap.oauth.expect("the grant persists");
    assert_eq!(grant.client_id, "client-xyz");
    assert_eq!(grant.refresh_token.expose(), "rt-1");

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
        ..Host::new("jmap-add-store-refused")
    };

    let failure = complete(
        &host,
        grant_document("alice@example.test", QUIET_TOKEN_ENDPOINT),
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
        host.store.stored(&jmap_id("alice@example.test")).is_none(),
        "the store holds no orphaned grant"
    );
    assert!(
        host.app.added.lock().unwrap().is_empty(),
        "the account never joined the app"
    );
}

#[test]
fn a_dead_grant_during_the_dial_is_the_dead_word() {
    let host = Host::new("jmap-add-dead-grant");

    let failure = complete(
        &host,
        grant_document("alice@example.test", QUIET_TOKEN_ENDPOINT),
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
        host.store.stored(&jmap_id("alice@example.test")).is_none(),
        "no credential survives a dial that never answered"
    );
}

#[test]
fn a_failed_dial_rolls_a_new_add_back_whole() {
    let host = Host::new("jmap-add-dial-failed");

    let failure = complete(
        &host,
        grant_document("alice@example.test", QUIET_TOKEN_ENDPOINT),
        failing_dial(Arc::new(DialSaw::default()), || {
            AccountError::Jmap("the session could not be read".to_owned())
        }),
    )
    .expect_err("the dial's failure is the completion's");

    assert_eq!(failure.refusal, RefusalKind::Indeterminate);
    assert!(
        failure.detail.contains("session"),
        "the dial's own sentence rides in the detail: {}",
        failure.detail
    );
    assert!(
        host.dials.ids().is_empty() && host.dials.indexed_ids().is_empty(),
        "the pre-registered entry is taken back"
    );
    assert!(
        host.store.stored(&jmap_id("alice@example.test")).is_none(),
        "no credential survives a dial that never answered"
    );
    assert!(host.app.added.lock().unwrap().is_empty());
}

#[test]
fn a_sign_in_for_an_account_that_stands_keeps_its_shape_and_rotates() {
    let host = Host::new("jmap-add-reconsent");
    register_stored(&host, standing_config(), "Alice@Example.test");
    let id = jmap_id("Alice@Example.test");

    let address = complete(
        &host,
        grant_document("Alice@Example.test", QUIET_TOKEN_ENDPOINT),
        ok_dial(Arc::new(DialSaw::default())),
    )
    .expect("the re-consent completes");

    assert_eq!(address, "Alice@Example.test");
    assert_eq!(
        stored_documents(&host, &id).len(),
        1,
        "the re-consent persists once, through the rotation path"
    );

    let document = host.store.stored(&id).expect("the renewed grant persists");
    let config = astra_account::load_str(&document).expect("the document loads");
    let jmap = config.jmap.expect("the jmap section stands");
    assert_eq!(
        jmap.oauth.expect("the grant").refresh_token.expose(),
        "rt-1",
        "the renewed grant replaced the old one"
    );
    assert!(jmap.password.is_none(), "the re-consent writes no password");
    assert_eq!(
        config.shape.id.map(|pinned| pinned.as_str().to_owned()),
        Some(id.clone()),
        "the stored shape keeps its pinned id"
    );
    assert!(
        config.dav.is_some(),
        "the dav section the grant does not own stays"
    );
    assert_eq!(
        config.certificate_exceptions.len(),
        1,
        "the accepted certificate the grant does not own stays"
    );

    let entry = host
        .dials
        .of(&id)
        .expect("the standing entry survives the re-consent")
        .0;
    assert_eq!(
        entry
            .jmap
            .expect("the jmap section")
            .oauth
            .expect("the grant")
            .refresh_token
            .expose(),
        "rt-1",
        "the dial table's entry carries the renewed grant"
    );
    let joined = wait_for(|| host.app.added.lock().unwrap().first().cloned());
    assert_eq!(joined, id, "the renewed account re-joins the app in place");
}

#[test]
fn a_failed_dial_on_a_reconsent_restores_the_standing_entry() {
    let host = Host::new("jmap-add-reconsent-dial-failed");
    register_stored(&host, standing_config(), "Alice@Example.test");
    let id = jmap_id("Alice@Example.test");

    complete(
        &host,
        grant_document("Alice@Example.test", QUIET_TOKEN_ENDPOINT),
        failing_dial(Arc::new(DialSaw::default()), || {
            AccountError::Jmap("the session could not be read".to_owned())
        }),
    )
    .expect_err("the failed dial refuses the re-consent");

    let entry = host.dials.of(&id).expect("the standing entry stays").0;
    assert_eq!(
        entry
            .jmap
            .expect("the jmap section")
            .oauth
            .expect("the grant")
            .refresh_token
            .expose(),
        "rt-old",
        "the entry is put back as it stood, not left on the new grant"
    );
}
