// SPDX-License-Identifier: GPL-3.0-only
//! The offline end-to-end proof of the Google account's whole life over
//! the scripted seams, the sibling of the Microsoft suite, split from
//! the boot tests for the 500-line cap the same way and sharing their
//! fixtures through `super`.
//!
//! What is real here and what is scripted, decided and recorded: the
//! engine's provider-google offers no offline replay (its live tests are
//! gated on a real access token), and a stored document's token endpoint
//! is pinned to the real Google authority, so a healthy dial or a
//! boot-time refresh cannot run offline. Every link that is reachable
//! offline runs the production code over real HTTP against scripted
//! servers on loopback: the granted-set-picks-the-endpoint userinfo read
//! ([`astra_account::fetch_google_address_at`]), the shared token
//! source's refresh and its rotation backstop through the real sink into
//! the real dial table and credential store
//! ([`production_google_source`]; Google does not rotate in practice,
//! the scripted ratcheting endpoint proves the path), the completion's
//! register-before-dial ordering, the boot's stored-account dial, and
//! the account-info seam the accounts page rows read. The dial seam
//! itself is scripted; its token boundary over a scripted source is what
//! the account layer's dial tests proved. The live proof over the real
//! Google APIs awaits the owner's Google Cloud registration, and nothing
//! here fakes it.
//!
//! [`production_google_source`]: crate::oauth::google_add::production_google_source

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use astra_app::HostAccountOps;
use astra_oauth::{OAuthProviderConfig, TokenSet};

use astra_account::AccountConfig;

use crate::app_state::AppPort;
use crate::commands_fakes::{scratch, FakeApp, FakeStore};
use crate::dial_table::AccountDials;
use crate::oauth::google_add::{
    complete_google_signin, production_google_source, GoogleAccountWiring, GoogleAddressLookup,
    GoogleDial,
};

use super::servers::{
    google_provider_config_at, google_token_set_with, profile_server, ratcheting_token_endpoint,
};

// The scripted dial seams, split from this file for the 500-line cap.
#[path = "google_lifecycle_tests/dials.rs"]
mod dials;

use dials::{boot_dial, minting_dial, BootAnswer, DialSaw};

/// The lookup seam over the production read: the real
/// [`astra_account::fetch_google_address_at`] against the scripted
/// userinfo server, the same function
/// [`production_google_lookup`] wraps around the constant roots. The
/// granted set picks the endpoint (this proof's grant holds
/// `userinfo.email`, so the userinfo read runs and the profile root is
/// never read; the same server stands behind both roots, so the pick is
/// the granted set's own doing). The only scripted part of this link is
/// the server.
///
/// [`production_google_lookup`]: crate::oauth::google_add::production_google_lookup
fn real_lookup(base: String) -> GoogleAddressLookup {
    Arc::new(move |access_token, granted| {
        let userinfo_root = base.clone();
        let profile_root = base.clone();
        let access_token = access_token.to_owned();
        let granted = granted.to_vec();
        Box::pin(async move {
            astra_account::fetch_google_address_at(
                &userinfo_root,
                &profile_root,
                &access_token,
                &granted,
            )
            .await
        })
    })
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
    fn wiring(&self) -> GoogleAccountWiring {
        GoogleAccountWiring {
            app: Arc::clone(&self.app) as Arc<dyn AppPort>,
            dials: Arc::clone(&self.dials),
            credentials: self.store.clone(),
            spawner: self.runtime.handle(),
            refusals: Arc::clone(&self.refusals),
        }
    }
}

/// The completion over the real seams, on the host's runtime: the
/// address the lookup named on success.
fn complete(
    host: &Host,
    tokens: TokenSet,
    provider: OAuthProviderConfig,
    lookup: GoogleAddressLookup,
    dial: GoogleDial,
) -> Result<String, String> {
    let wiring = host.wiring();
    host.runtime
        .block_on(move || {
            complete_google_signin(
                wiring,
                tokens,
                provider,
                lookup,
                dial,
                production_google_source(),
            )
        })
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
fn google_id(email: &str) -> String {
    format!("{}@google", email.trim().to_lowercase())
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

/// The whole healthy life: the sign-in completes through the real
/// userinfo read (the granted set picking the endpoint) and the real
/// source over the scripted endpoint, the rotation the dial's mint
/// produced lands in the entry and the store, the account joins the app,
/// and the next boot dials over the grant the rotation left, never the
/// superseded one.
#[test]
fn the_sign_in_completes_the_rotation_lands_and_the_next_boot_dials_the_advanced_grant() {
    let host = Host::new("google-life-healthy");
    let (userinfo_base, userinfo_seen) = profile_server(r#"{"email":"Alice@Gmail.Example"}"#);
    let (endpoint, hits) = ratcheting_token_endpoint("fresh-refresh");
    let saw = Arc::new(DialSaw::default());
    let id = google_id("Alice@Gmail.Example");

    let address = complete(
        &host,
        google_token_set_with("fresh-refresh", "fresh-access"),
        google_provider_config_at(&endpoint),
        real_lookup(userinfo_base),
        minting_dial(Arc::clone(&saw), Arc::clone(&host.dials), "fresh-access"),
    )
    .expect("the sign-in completes into an account");

    // The lookup link: the real read, authenticated by the token the
    // exchange just minted, naming the account over the endpoint the
    // granted set picked (userinfo.email held, so the userinfo read and
    // never the Gmail profile).
    assert_eq!(address, "Alice@Gmail.Example");
    let head = userinfo_seen
        .lock()
        .unwrap()
        .first()
        .cloned()
        .unwrap_or_default();
    assert!(
        head.lines()
            .any(|line| line.starts_with("GET /oauth2/v3/userinfo ")),
        "the granted set picked the userinfo endpoint, never the profile: {head}"
    );
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

    // The store link: every persisted document carries the advanced grant
    // and the Desktop secret beside it.
    let documents = stored_documents(&host, &id);
    assert!(
        documents.len() >= 2,
        "the during-dial rotation and the completion each persisted: {}",
        documents.len()
    );
    for document in &documents {
        let google = astra_account::load_str(document)
            .expect("each stored document loads")
            .google
            .expect("each document is a google one");
        assert_eq!(
            google.refresh_token.expose(),
            "rotated-1",
            "a persisted document restored the superseded grant"
        );
        assert_eq!(
            google
                .client_secret
                .as_ref()
                .map(astra_account::Secret::expose),
            Some("gsecret"),
            "each persisted document keeps the secret a launch-later refresh sends again"
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
                super::google_boot::unused_graph_dial(),
                dial,
                &refusals,
            )
            .await
        })
    };
    assert_eq!(dialled.len(), 1, "the stored account dialled at boot");
    assert_eq!(
        dialled[0].account.identity.email, "Alice@Gmail.Example",
        "the boot names the account by the address the lookup found"
    );
    assert_eq!(
        dialled[0].account.providers.len(),
        2,
        "the account-global mail provider stands and the calendar appends after it"
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
        astra_viewmodel::AccountKind::Google,
        "the row names the Google kind, never an imap one"
    );
    assert_eq!(info.address, "Alice@Gmail.Example");
    assert_eq!(info.grant, None, "the healthy row carries no grant state");
}

/// The death and the way back: a standing account whose grant the server
/// refused skips the boot with the expired-sign-in state reaching the
/// accounts row, and signing in again through the same seams lands the
/// renewed grant and clears the row.
#[test]
fn a_dead_grant_s_classification_reaches_the_accounts_row_and_a_re_sign_in_clears_it() {
    let host = Host::new("google-life-dead");
    let (userinfo_base, _seen) = profile_server(r#"{"email":"Alice@Gmail.Example"}"#);
    let (endpoint, _hits) = ratcheting_token_endpoint("fresh-refresh");
    let id = google_id("Alice@Gmail.Example");

    complete(
        &host,
        google_token_set_with("fresh-refresh", "fresh-access"),
        google_provider_config_at(&endpoint),
        real_lookup(userinfo_base.clone()),
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
                super::google_boot::unused_graph_dial(),
                dial,
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
        astra_viewmodel::AccountKind::Google,
        "the row names the Google kind, never an imap one"
    );
    assert_eq!(info.address, "Alice@Gmail.Example");
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
        google_token_set_with("regranted-refresh", "regranted-access"),
        google_provider_config_at(&regrant_endpoint),
        real_lookup(userinfo_base),
        minting_dial(
            Arc::new(DialSaw::default()),
            Arc::clone(&host.dials),
            "regranted-access",
        ),
    )
    .expect("the re-sign-in completes");
    assert_eq!(address, "Alice@Gmail.Example");

    let standing = standing_document(&host, &id);
    assert_eq!(
        standing
            .google
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
