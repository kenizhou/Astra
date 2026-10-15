// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the completion's account half: a token set plus the address
//! lookup build the `[graph]` document and add the account (the store's
//! refusal rolls the add back), a sign-in for an account that stands keeps
//! the stored shape and persists through the rotation path, a dial failure
//! rolls the registration back, and the pre-register ordering lands a
//! rotation that happens during the dial in the sink, so the grant that
//! persists is the advanced one. Every seam is a stand-in: no test touches
//! a network, and the first sync waits on the runtime handle the wiring
//! spawns on.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine_api::{AccountId, CalendarWrites, Capabilities, MailboxWrites, Provider};
use engine_provider::ConnectionInfo;

use astra_oauth::{OAuthProviderConfig, Secret, TokenSet};

use astra_account::{AccountConfig, AccountError, GraphTokenSource};

use crate::app_state::{AppPort, HostProvider};
use crate::commands_fakes::{scratch, FakeApp, FakeStore};
use crate::dial_table::AccountDials;
use crate::oauth::commands::RefusalKind;

use super::{
    complete_microsoft_signin, production_source, AddressLookup, GraphAccountWiring,
    GraphAddFailure, GraphDial, SignedInAccount, SourceBuilder,
};

#[path = "graph_add_tests/document_tests.rs"]
mod document_tests;
#[path = "graph_add_tests/rotation_tests.rs"]
mod rotation_tests;
#[path = "graph_add_tests/sink_tests.rs"]
mod sink_tests;

/// A provider stand-in: no served data, only the capability it advertises.
struct FakeGraph {
    calendars: bool,
}

impl FakeGraph {
    /// The calendar leg's stand-in.
    fn calendar() -> HostProvider {
        Box::new(Self { calendars: true })
    }

    /// One mail folder's stand-in.
    fn mail() -> HostProvider {
        Box::new(Self { calendars: false })
    }
}

#[async_trait]
impl Provider for FakeGraph {
    fn connection_info(&self) -> ConnectionInfo {
        let caps = if self.calendars {
            Capabilities::none().with_calendars()
        } else {
            Capabilities::none().with_mail()
        };
        ConnectionInfo::new(caps)
    }
}

impl MailboxWrites for FakeGraph {}
impl CalendarWrites for FakeGraph {}

/// The flow's provider config: the registration the client minted the
/// request over.
fn provider_config() -> OAuthProviderConfig {
    OAuthProviderConfig::microsoft(
        "client-abc",
        "common",
        "com.kenizhou.astra://oauth",
        &[
            "offline_access",
            "https://graph.microsoft.com/Mail.ReadWrite",
            "https://graph.microsoft.com/Calendars.ReadWrite",
        ],
    )
}

/// The token set a completed exchange answers: a fresh access token, a
/// fresh refresh token, and a scope that granted one scope less than was
/// asked for.
fn token_set() -> TokenSet {
    TokenSet {
        access_token: Secret::new("fresh-access".to_owned()),
        refresh_token: Some(Secret::new("fresh-refresh".to_owned())),
        expires_at: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        scope: "offline_access https://graph.microsoft.com/Mail.ReadWrite".to_owned(),
        token_type: "Bearer".to_owned(),
    }
}

/// The granted set the fixture's scope string names.
fn granted_set() -> Vec<String> {
    vec![
        "offline_access".to_owned(),
        "https://graph.microsoft.com/Mail.ReadWrite".to_owned(),
    ]
}

/// The lookup seam answering one fixed address.
fn lookup_seam(address: &str) -> AddressLookup {
    let address = address.to_owned();
    Arc::new(move |_token| {
        let address = address.clone();
        Box::pin(async move { Ok(address) })
    })
}

/// What one scripted dial saw: the account id, the refresh token the
/// registered entry carried at dial time, and the token source it was
/// handed.
#[derive(Default)]
struct DialSaw {
    ids: Mutex<Vec<String>>,
    refresh_tokens: Mutex<Vec<String>>,
}

/// The dial seam answering two mail providers and a calendar, recording
/// what it saw.
fn ok_dial(saw: Arc<DialSaw>) -> GraphDial {
    dial_seam(saw, |_| async {
        Ok(astra_account::MicrosoftProviders {
            mail: vec![FakeGraph::mail(), FakeGraph::mail()],
            calendar: Some(FakeGraph::calendar()),
            calendar_error: None,
            calendar_reauth_required: false,
        })
    })
}

/// The dial seam over one scripted answer.
fn dial_seam<F, Fut>(saw: Arc<DialSaw>, answer: F) -> GraphDial
where
    F: Fn(Arc<GraphTokenSource>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<astra_account::MicrosoftProviders, AccountError>> + Send + 'static,
{
    Arc::new(move |account, tokens, config| {
        let answer = answer(Arc::clone(&tokens));
        let account = account.as_str().to_owned();
        let refresh = config
            .graph
            .as_ref()
            .map(|graph| graph.refresh_token.expose().to_owned())
            .unwrap_or_default();
        let saw = Arc::clone(&saw);
        Box::pin(async move {
            saw.ids.lock().unwrap().push(account);
            saw.refresh_tokens.lock().unwrap().push(refresh);
            answer.await
        })
    })
}

/// A dial seam that always fails with one sentence.
fn failing_dial(saw: Arc<DialSaw>, err: &'static str) -> GraphDial {
    dial_seam(saw, move |_| async move {
        Err(AccountError::Graph(err.to_owned()))
    })
}

/// A mock token endpoint that rotates the refresh token on every refresh,
/// exactly the server shape that makes a rotation during a dial matter.
struct Host {
    app: Arc<FakeApp>,
    store: Arc<FakeStore>,
    dials: Arc<AccountDials>,
    runtime: crate::runtime::AppRuntime,
}

impl Host {
    /// The pieces over a fresh scratch index.
    fn new(name: &str) -> Self {
        Self {
            app: FakeApp::new(),
            store: FakeStore::accepting(),
            dials: Arc::new(AccountDials::empty(&scratch(name))),
            runtime: crate::runtime::build_runtime().expect("the test runtime builds"),
        }
    }

    /// The completion's wiring over these pieces.
    fn wiring(&self) -> GraphAccountWiring {
        GraphAccountWiring {
            app: Arc::clone(&self.app) as Arc<dyn AppPort>,
            dials: Arc::clone(&self.dials),
            credentials: self.store.clone(),
            spawner: self.runtime.handle(),
        }
    }
}

/// The completion over the given seams, on the host's runtime.
fn complete(
    host: &Host,
    lookup: AddressLookup,
    dial: GraphDial,
    source: SourceBuilder,
) -> Result<SignedInAccount, GraphAddFailure> {
    let wiring = host.wiring();
    let tokens = token_set();
    let provider = provider_config();
    host.runtime
        .block_on(move || complete_microsoft_signin(wiring, tokens, provider, lookup, dial, source))
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

/// The account id a lookup of `email` derives: the address under the one
/// protocol the account dials.
fn graph_id(email: &str) -> String {
    format!("{}@graph", email.trim().to_lowercase())
}

/// The stored document's `[graph]` section, parsed back.
fn stored_graph(host: &Host, id: &str) -> astra_account::GraphAccount {
    let document = host
        .store
        .stored(id)
        .unwrap_or_else(|| panic!("the store holds the document for [{id}]"));
    astra_account::load_str(&document)
        .expect("the stored document loads")
        .graph
        .expect("the document is a graph one")
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

/// A stored graph document for the re-consent tests: an old grant, a
/// pinned id, and an accepted certificate the grant does not own.
fn stored_config() -> AccountConfig {
    astra_account::load_str(concat!(
        "id = \"alice@example.com@graph\"\n",
        "[graph]\n",
        "email = \"Alice@Example.com\"\n",
        "client_id = \"client-abc\"\n",
        "tenant = \"common\"\n",
        "redirect_uri = \"com.kenizhou.astra://oauth\"\n",
        "scopes = [\"offline_access\"]\n",
        "refresh_token = \"old-refresh\"\n",
        "\n[[certificate_exception]]\n",
        "server_name = \"graph.microsoft.com\"\n",
        "sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n",
    ))
    .expect("the stored fixture loads")
}

/// A stored graph document with the given account's email, the
/// offline_access-only grant, and the granted set already recorded.
fn register_stored(host: &Host, config: AccountConfig, email: &str) {
    let id = AccountId::try_from(graph_id(email).as_str()).unwrap();
    host.dials
        .register(&id, config, astra_account::ImapConnections::new())
        .expect("the standing account registers");
}

#[test]
fn a_token_set_and_the_address_lookup_build_the_document_and_add_the_account() {
    let host = Host::new("graph-add-ok");
    let saw = Arc::new(DialSaw::default());

    let added = complete(
        &host,
        lookup_seam("Alice@Example.com"),
        ok_dial(Arc::clone(&saw)),
        production_source(),
    )
    .expect("the sign-in completes into an account");

    assert_eq!(added.address, "Alice@Example.com");
    assert!(!added.calendar_withheld, "the calendar connected");
    let id = graph_id("Alice@Example.com");
    assert_eq!(
        saw.ids.lock().unwrap().as_slice(),
        [id.as_str()],
        "the dial ran for the derived id"
    );
    assert_eq!(
        saw.refresh_tokens.lock().unwrap().as_slice(),
        ["fresh-refresh"],
        "the registered entry the dial read carries the fresh grant"
    );

    let graph = stored_graph(&host, &id);
    assert_eq!(graph.email, "Alice@Example.com");
    assert_eq!(graph.client_id, "client-abc");
    assert_eq!(graph.tenant, "common");
    assert_eq!(graph.redirect_uri, "com.kenizhou.astra://oauth");
    assert_eq!(
        graph.scopes,
        provider_config().scopes,
        "the document names the scopes the flow asked for"
    );
    assert_eq!(graph.refresh_token.expose(), "fresh-refresh");
    assert_eq!(
        graph.granted_scopes,
        Some(granted_set()),
        "the granted set is the response's own, one scope narrower than the ask"
    );

    assert_eq!(
        host.dials.ids(),
        vec![id.clone()],
        "the account is registered"
    );
    let joined = wait_for(|| host.app.added.lock().unwrap().first().cloned());
    assert_eq!(joined, id, "the account joined the app");
    let synced = wait_for(|| host.app.synced.lock().unwrap().first().cloned());
    assert_eq!(synced, id, "the first sync runs in the background");
}

#[test]
fn a_refused_store_rolls_the_whole_add_back() {
    let host = Host {
        store: FakeStore::refusing(),
        ..Host::new("graph-add-store-refused")
    };

    let failure = complete(
        &host,
        lookup_seam("alice@example.test"),
        ok_dial(Arc::new(DialSaw::default())),
        production_source(),
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
        host.store.stored(&graph_id("alice@example.test")).is_none(),
        "the store holds no orphaned grant"
    );
    assert!(
        host.app.added.lock().unwrap().is_empty(),
        "the account never joined the app"
    );
}

#[test]
fn a_sign_in_for_an_account_that_stands_keeps_its_shape_and_rotates() {
    let host = Host::new("graph-add-reconsent");
    register_stored(&host, stored_config(), "Alice@Example.com");
    let id = graph_id("Alice@Example.com");

    let added = complete(
        &host,
        lookup_seam("Alice@Example.com"),
        ok_dial(Arc::new(DialSaw::default())),
        production_source(),
    )
    .expect("the re-consent completes");

    assert_eq!(added.address, "Alice@Example.com");
    assert_eq!(
        stored_documents(&host, &id).len(),
        1,
        "the re-consent persists once, through the rotation path"
    );

    let document = host.store.stored(&id).expect("the renewed grant persists");
    let config = astra_account::load_str(&document).expect("the document loads");
    let graph = config.graph.expect("the graph section stands");
    assert_eq!(
        graph.refresh_token.expose(),
        "fresh-refresh",
        "the renewed grant replaced the old one"
    );
    assert_eq!(graph.granted_scopes, Some(granted_set()));
    assert_eq!(
        config.shape.id.map(|pinned| pinned.as_str().to_owned()),
        Some(id.clone()),
        "the stored shape keeps its pinned id"
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
            .graph
            .expect("the entry's grant")
            .refresh_token
            .expose(),
        "fresh-refresh",
        "the dial table's entry carries the renewed grant"
    );
    let joined = wait_for(|| host.app.added.lock().unwrap().first().cloned());
    assert_eq!(joined, id, "the renewed account re-joins the app in place");
}

#[test]
fn a_failed_dial_rolls_a_new_add_back_whole() {
    let host = Host::new("graph-add-dial-failed");

    let failure = complete(
        &host,
        lookup_seam("alice@example.test"),
        failing_dial(Arc::new(DialSaw::default()), "listing mailboxes: refused"),
        production_source(),
    )
    .expect_err("the dial's failure is the completion's");

    assert_eq!(failure.refusal, RefusalKind::Indeterminate);
    assert!(
        failure.detail.contains("listing mailboxes"),
        "the dial's own sentence rides in the detail: {}",
        failure.detail
    );
    assert!(
        host.dials.ids().is_empty() && host.dials.indexed_ids().is_empty(),
        "the pre-registered entry is taken back"
    );
    assert!(
        host.store.stored(&graph_id("alice@example.test")).is_none(),
        "no credential survives a dial that never answered"
    );
    assert!(host.app.added.lock().unwrap().is_empty());
}

#[test]
fn a_failed_dial_on_a_reconsent_restores_the_standing_entry() {
    let host = Host::new("graph-add-reconsent-dial-failed");
    register_stored(&host, stored_config(), "Alice@Example.com");
    let id = graph_id("Alice@Example.com");

    complete(
        &host,
        lookup_seam("Alice@Example.com"),
        failing_dial(Arc::new(DialSaw::default()), "listing mailboxes: refused"),
        production_source(),
    )
    .expect_err("the failed dial refuses the re-consent");

    let entry = host.dials.of(&id).expect("the standing entry stays").0;
    assert_eq!(
        entry
            .graph
            .expect("the graph section stands")
            .refresh_token
            .expose(),
        "old-refresh",
        "the entry is put back as it stood, not left on the new grant"
    );
}

#[cfg(test)]
#[path = "graph_add_tests/door_tests.rs"]
mod door;
