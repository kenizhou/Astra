// SPDX-License-Identifier: GPL-3.0-only
//! The offline end-to-end proof of the Microsoft account's whole life over
//! the scripted seams, split from the boot tests for the 500-line cap and
//! sharing their fixtures through `super`.
//!
//! What is real here and what is scripted, decided and recorded: the
//! engine's provider-graph offers no offline replay (its live tests are
//! gated on a real access token, its fixtures are recorded bodies for the
//! parsers), and a stored document's token endpoint is pinned to the real
//! Microsoft authority through its tenant, so a healthy folder list or a
//! boot-time refresh cannot run offline. Every link that is reachable
//! offline runs the production code over real HTTP against scripted
//! servers on loopback: the `/me` address lookup
//! ([`astra_account::fetch_primary_address_at`]), the token source's
//! refresh and its rotation through the real sink into the real dial table
//! and credential store ([`production_source`]), the completion's
//! register-before-dial ordering, the boot's stored-account dial, and the
//! account-info seam the accounts page rows read. The dial seam itself is
//! scripted; its token boundary over a scripted source is what the account
//! layer's dial tests proved. The live proof over the real Graph awaits
//! the Azure registration, and nothing here fakes it.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use astra_app::HostAccountOps;
use astra_oauth::{OAuthProviderConfig, TokenSet};

use astra_account::AccountConfig;

use crate::app_state::AppPort;
use crate::commands_fakes::{scratch, FakeApp, FakeStore};
use crate::dial_table::AccountDials;
use crate::oauth::graph_add::{
    complete_microsoft_signin, production_source, AddressLookup, GraphAccountWiring, GraphDial,
};

use super::FakeDav;

#[path = "graph_lifecycle_tests/servers.rs"]
mod servers;

use servers::{me_server, provider_config_at, ratcheting_token_endpoint, token_set_with};

/// The lookup seam over the production read: the real
/// [`astra_account::fetch_primary_address_at`] against the scripted `/me`
/// server, the same function [`production_lookup`] wraps around the
/// constant root. The only scripted part of this link is the server.
///
/// [`production_lookup`]: crate::oauth::graph_add::production_lookup
fn real_lookup(base: String) -> AddressLookup {
    Arc::new(move |access_token| {
        let base = base.clone();
        let access_token = access_token.to_owned();
        Box::pin(async move { astra_account::fetch_primary_address_at(&base, &access_token).await })
    })
}

/// What one lifecycle dial observed: the grant the registered entry
/// carried when the dial began, the access token the mint answered, and
/// the grant the entry carried after it.
#[derive(Default)]
struct DialSaw {
    entry_before: Mutex<Vec<String>>,
    minted: Mutex<Vec<String>>,
    entry_after: Mutex<Vec<String>>,
}

/// The dial seam that mints through the real source, exactly what the
/// dial's first provider call does: the server has moved past the access
/// token the exchange issued, so the mint refreshes against the scripted
/// endpoint and any rotation the answer carries lands through the real
/// sink while the dial is still running.
fn minting_dial(
    saw: Arc<DialSaw>,
    dials: Arc<AccountDials>,
    seeded_access: &'static str,
) -> GraphDial {
    Arc::new(move |account, tokens, config| {
        let before = config
            .graph
            .as_ref()
            .map(|graph| graph.refresh_token.expose().to_owned())
            .unwrap_or_default();
        let account = account.as_str().to_owned();
        let saw = Arc::clone(&saw);
        let dials = Arc::clone(&dials);
        Box::pin(async move {
            saw.entry_before.lock().unwrap().push(before);
            let minted = tokens.access_token_replacing(seeded_access).await?;
            saw.minted.lock().unwrap().push(minted);
            let after = dials
                .of(&account)
                .and_then(|(stored, _)| {
                    stored
                        .graph
                        .map(|graph| graph.refresh_token.expose().to_owned())
                })
                .unwrap_or_default();
            saw.entry_after.lock().unwrap().push(after);
            Ok(astra_account::MicrosoftProviders {
                mail: vec![FakeDav::mail(), FakeDav::mail()],
                calendar: Some(FakeDav::calendar()),
                calendar_error: None,
                calendar_reauth_required: false,
            })
        })
    })
}

/// The boot's dial seam over one scripted answer for a grant it records,
/// the healthy re-dial's stand-in: the real boot machinery builds the
/// source and reads the stored document, and the seam answers what the
/// folder list would.
fn boot_dial(grants: Arc<Mutex<Vec<String>>>, answer: BootAnswer) -> GraphDial {
    Arc::new(move |_account, _tokens, config| {
        let grant = config
            .graph
            .as_ref()
            .map(|graph| graph.refresh_token.expose().to_owned())
            .unwrap_or_default();
        let grants = Arc::clone(&grants);
        Box::pin(async move {
            grants.lock().unwrap().push(grant);
            match answer {
                BootAnswer::Healthy => Ok(astra_account::MicrosoftProviders {
                    mail: vec![FakeDav::mail()],
                    calendar: Some(FakeDav::calendar()),
                    calendar_error: None,
                    calendar_reauth_required: false,
                }),
                // The dead word the real token source answers for a grant
                // the server refused, proven over HTTP at the token source;
                // a stored document's endpoint is pinned to the real
                // authority, so the boot leg scripts the arrival.
                BootAnswer::DeadGrant => Err(astra_account::AccountError::SigninRejected(
                    "the refresh token was refused".to_owned(),
                )),
            }
        })
    })
}

/// What the boot's dial seam answers.
#[derive(Clone, Copy)]
enum BootAnswer {
    /// A dialled account with mail and calendar.
    Healthy,
    /// A grant the token endpoint refused.
    DeadGrant,
}

/// The host halves the lifecycle drives: the same pieces a running app
/// holds, the store faked because the proof owns no OS keyring.
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
    fn wiring(&self) -> GraphAccountWiring {
        GraphAccountWiring {
            app: Arc::clone(&self.app) as Arc<dyn AppPort>,
            dials: Arc::clone(&self.dials),
            credentials: self.store.clone(),
            spawner: self.runtime.handle(),
            refusals: Arc::clone(&self.refusals),
        }
    }
}

/// The completion over the real seams, on the host's runtime.
fn complete(
    host: &Host,
    tokens: TokenSet,
    provider: OAuthProviderConfig,
    lookup: AddressLookup,
    dial: GraphDial,
) -> Result<String, String> {
    let wiring = host.wiring();
    host.runtime
        .block_on(move || {
            complete_microsoft_signin(wiring, tokens, provider, lookup, dial, production_source())
        })
        .map(|added| added.address)
        .map_err(|failure| failure.detail)
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

/// The account id a lookup of `email` derives.
fn graph_id(email: &str) -> String {
    format!("{}@graph", email.trim().to_lowercase())
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

/// The last document the store holds for `id`: the standing grant.
fn standing_document(host: &Host, id: &str) -> AccountConfig {
    let document = stored_documents(host, id)
        .into_iter()
        .next_back()
        .unwrap_or_else(|| panic!("the store holds the document for [{id}]"));
    astra_account::load_str(&document).expect("the standing document loads")
}

/// The account-info rows the accounts page reads, over the host's own
/// table and refusal map.
fn account_infos(host: &Host) -> Vec<astra_app::AccountInfo> {
    let ops = crate::account_ops::HostAccountStore::new(
        Arc::clone(&host.dials),
        host.store.clone(),
        Arc::clone(&host.refusals),
    );
    host.runtime
        .block_on(move || async move { ops.account_infos().await })
}

/// The whole healthy life: the sign-in completes through the real `/me`
/// lookup and the real source over the scripted endpoint, the rotation
/// the dial's mint produced lands in the entry and the store, the account
/// joins the app, and the next boot dials over the grant the rotation
/// left, never the superseded one.
#[test]
fn the_sign_in_completes_the_rotation_lands_and_the_next_boot_dials_the_advanced_grant() {
    let host = Host::new("graph-life-healthy");
    let (me_base, me_seen) = me_server(
        r#"{"mail":"Alice@Example.com","userPrincipalName":"alice@tenant.example.test"}"#,
    );
    let (endpoint, hits) = ratcheting_token_endpoint("fresh-refresh");
    let saw = Arc::new(DialSaw::default());
    let id = graph_id("Alice@Example.com");

    let address = complete(
        &host,
        token_set_with("fresh-refresh", "fresh-access"),
        provider_config_at(&endpoint),
        real_lookup(me_base),
        minting_dial(Arc::clone(&saw), Arc::clone(&host.dials), "fresh-access"),
    )
    .expect("the sign-in completes into an account");

    // The lookup link: the real read, authenticated by the token the
    // exchange just minted, naming the account.
    assert_eq!(address, "Alice@Example.com");
    let head = me_seen.lock().unwrap().first().cloned().unwrap_or_default();
    assert!(
        head.lines()
            .any(|line| line.eq_ignore_ascii_case("Authorization: Bearer fresh-access")),
        "the lookup ran with the exchange's access token: {head}"
    );
    // The source link: exactly one refresh served the dial's mint.
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "the dial refreshed more than once against the scripted endpoint"
    );
    assert_eq!(
        saw.minted.lock().unwrap().as_slice(),
        ["AT-1"],
        "the mint answered the rotated generation"
    );
    assert_eq!(
        saw.entry_before.lock().unwrap().as_slice(),
        ["fresh-refresh"],
        "the dial read the entry the completion registered before the dial"
    );
    assert_eq!(
        saw.entry_after.lock().unwrap().as_slice(),
        ["rotated-1"],
        "the rotation landed in the entry while the dial was still running"
    );

    // The store link: every persisted document carries the advanced grant.
    let documents = stored_documents(&host, &id);
    assert!(
        documents.len() >= 2,
        "the during-dial rotation and the completion each persisted: {}",
        documents.len()
    );
    for document in &documents {
        let graph = astra_account::load_str(document)
            .expect("each stored document loads")
            .graph
            .expect("each document is a graph one");
        assert_eq!(
            graph.refresh_token.expose(),
            "rotated-1",
            "a persisted document restored the superseded grant"
        );
    }

    // The app link: the account joins and its first sync runs.
    let joined = wait_for(|| host.app.added.lock().unwrap().first().cloned());
    assert_eq!(joined, id, "the account joined the app");
    let synced = wait_for(|| host.app.synced.lock().unwrap().first().cloned());
    assert_eq!(synced, id, "the first sync runs in the background");

    // The boot link: the stored account re-dials through the real boot
    // machinery over the grant the rotation left.
    let boot_grants = Arc::new(Mutex::new(Vec::new()));
    let dialled = {
        let dials = Arc::clone(&host.dials);
        let store: Arc<dyn crate::credential_store::AccountCredentialStore> = host.store.clone();
        let dial = boot_dial(Arc::clone(&boot_grants), BootAnswer::Healthy);
        let refusals = Arc::clone(&host.refusals);
        host.runtime.block_on(move || async move {
            super::super::stored::dial_stored(
                &dials,
                &store,
                dial,
                super::google_boot::unused_dial(),
                &refusals,
            )
            .await
        })
    };
    assert_eq!(dialled.len(), 1, "the stored account dialled at boot");
    assert_eq!(
        dialled[0].account.identity.email, "Alice@Example.com",
        "the boot names the account by the address the lookup found"
    );
    assert_eq!(
        boot_grants.lock().unwrap().as_slice(),
        ["rotated-1"],
        "the boot dialled over the grant the rotation landed, not the superseded one"
    );
    assert_eq!(
        host.refusals.of(&id),
        None,
        "a healthy dial crosses no refusal state"
    );
    let infos = account_infos(&host);
    let info = infos
        .iter()
        .find(|info| info.id.as_str() == id)
        .expect("the accounts page row stands");
    assert_eq!(
        info.kind,
        astra_viewmodel::AccountKind::Microsoft,
        "the row names the Microsoft kind"
    );
    assert_eq!(info.address, "Alice@Example.com");
    assert_eq!(info.grant, None, "the healthy row carries no grant state");
}

/// The death and the way back: a standing account whose grant the server
/// refused skips the boot with the expired-sign-in state reaching the
/// accounts row, and signing in again through the same seams lands the
/// renewed grant and clears the row.
#[test]
fn a_dead_grant_s_classification_reaches_the_accounts_row_and_a_re_sign_in_clears_it() {
    let host = Host::new("graph-life-dead");
    let (me_base, _seen) = me_server(
        r#"{"mail":"Alice@Example.com","userPrincipalName":"alice@tenant.example.test"}"#,
    );
    let (endpoint, _hits) = ratcheting_token_endpoint("fresh-refresh");
    let id = graph_id("Alice@Example.com");

    complete(
        &host,
        token_set_with("fresh-refresh", "fresh-access"),
        provider_config_at(&endpoint),
        real_lookup(me_base.clone()),
        minting_dial(
            Arc::new(DialSaw::default()),
            Arc::clone(&host.dials),
            "fresh-access",
        ),
    )
    .expect("the account stands");

    // The grant dies: the boot's dial classifies it dead and the account
    // skips, leaving the refusal state as the only account of why.
    let dialled = {
        let dials = Arc::clone(&host.dials);
        let store: Arc<dyn crate::credential_store::AccountCredentialStore> = host.store.clone();
        let dial = boot_dial(Arc::new(Mutex::new(Vec::new())), BootAnswer::DeadGrant);
        let refusals = Arc::clone(&host.refusals);
        host.runtime.block_on(move || async move {
            super::super::stored::dial_stored(
                &dials,
                &store,
                dial,
                super::google_boot::unused_dial(),
                &refusals,
            )
            .await
        })
    };
    assert!(dialled.is_empty(), "the account never joins the app");
    assert_eq!(
        host.refusals.of(&id),
        Some(astra_viewmodel::GrantState::SignInExpired),
        "the boot classified the dead grant for the surfaces"
    );
    let infos = account_infos(&host);
    let info = infos
        .iter()
        .find(|info| info.id.as_str() == id)
        .expect("the skipped account still renders its row");
    assert_eq!(
        info.kind,
        astra_viewmodel::AccountKind::Microsoft,
        "the row names the Microsoft kind, never an imap one"
    );
    assert_eq!(info.address, "Alice@Example.com");
    assert_eq!(
        info.grant,
        Some(astra_viewmodel::GrantState::SignInExpired),
        "the dead grant's classification reaches the accounts page row"
    );

    // The way back: the row's action, signing in again, over the same real
    // seams with a renewed grant.
    let (regrant_endpoint, _regrant_hits) = ratcheting_token_endpoint("regranted-refresh");
    let address = complete(
        &host,
        token_set_with("regranted-refresh", "regranted-access"),
        provider_config_at(&regrant_endpoint),
        real_lookup(me_base),
        minting_dial(
            Arc::new(DialSaw::default()),
            Arc::clone(&host.dials),
            "regranted-access",
        ),
    )
    .expect("the re-sign-in completes");
    assert_eq!(address, "Alice@Example.com");

    let standing = standing_document(&host, &id);
    assert_eq!(
        standing
            .graph
            .expect("the standing grant")
            .refresh_token
            .expose(),
        "rotated-1",
        "the renewed grant replaced the dead one in the store"
    );
    assert_eq!(
        host.refusals.of(&id),
        None,
        "the fresh grant clears the expired-sign-in row"
    );
    let infos = account_infos(&host);
    let info = infos
        .iter()
        .find(|info| info.id.as_str() == id)
        .expect("the row stands after the re-grant");
    assert_eq!(
        info.grant, None,
        "the accounts page row reads healthy again"
    );
    let rejoined = wait_for(|| host.app.added.lock().unwrap().get(1).cloned());
    assert_eq!(rejoined, id, "the renewed account re-joins the app");
}
