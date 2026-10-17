// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the JMAP OAuth leg's completion and token source: the grant
//! the exchange persists (the endpoints, the client id, the refresh
//! token, and no long-lived password), the refusals that protect an
//! account from a grant worth less than it looks, and the rotation write
//! through the host sink (the FASTMAIL ratchet rule).

use std::sync::{Arc, Mutex};

use super::test_support::{
    callback_for, discovery_server, full_chain, no_refresh_chain, now, stored_oauth_account,
    REDIRECT_URI,
};
use super::{complete_jmap_login, jmap_token_source, PendingJmapLogin};
use crate::graph::token_source::test_support::ratcheting_token_endpoint;
use crate::{load_str, AccountError, CredentialOrigin, JmapAccount, TokenSink};
use engine_api::AccountId;

#[tokio::test]
async fn complete_persists_the_grant_and_no_stored_secret() {
    let (base, _seen) = discovery_server(full_chain);
    let start = super::begin_jmap_login("me@example.net", Some(&base), REDIRECT_URI)
        .await
        .expect("a sign-in to begin");
    let toml = complete_jmap_login(
        &start.pending,
        &callback_for(&start.pending, "code-1"),
        now(),
    )
    .await
    .expect("a completed sign-in");

    let config = load_str(&toml).expect("a valid jmap document");
    let section = config.jmap.as_ref().expect("the jmap section");
    // An OAuth account stores no long-lived password: only the grant.
    assert!(section.password.is_none(), "{toml}");
    let grant = section.oauth.as_ref().expect("the persisted grant");
    assert_eq!(grant.client_id, "client-xyz");
    assert_eq!(grant.refresh_token.expose(), "rt-1");
    assert_eq!(grant.authorize_endpoint, format!("{base}/oauth/authorize"));
    assert_eq!(grant.token_endpoint, format!("{base}/token"));
    // The redirect URI must survive verbatim: a refresh replays it, and a
    // server that sees a different one rejects the grant.
    assert_eq!(grant.redirect_uri, REDIRECT_URI);
    assert_eq!(
        grant.resource.as_deref(),
        Some(format!("{base}/jmap/session").as_str()),
    );
    // Only when the server said it sends one (RFC 9207).
    assert_eq!(grant.issuer.as_deref(), Some(base.as_str()));
    assert!(grant.scopes.contains(&"offline_access".to_owned()));

    // And the document round-trips: the grant survives a load+save, where
    // the block this section replaces was stripped.
    let rewritten = config.to_toml().expect("serialisable");
    assert!(
        rewritten.contains("refresh_token = \"rt-1\""),
        "{rewritten}"
    );
    let reloaded = load_str(&rewritten).expect("a valid jmap document");
    assert_eq!(
        reloaded
            .jmap
            .as_ref()
            .expect("the jmap section")
            .oauth
            .as_ref()
            .expect("the round-tripped grant")
            .client_id,
        "client-xyz",
    );
}

#[tokio::test]
async fn a_server_that_issues_no_refresh_token_is_refused() {
    // `offline_access` is always requested, so a missing refresh token
    // means the account would break about an hour after setup: refused
    // here, where the reason can still be seen.
    let (base, _seen) = discovery_server(no_refresh_chain);
    let start = super::begin_jmap_login("me@example.net", Some(&base), REDIRECT_URI)
        .await
        .expect("a sign-in to begin");
    let err = complete_jmap_login(
        &start.pending,
        &callback_for(&start.pending, "code-1"),
        now(),
    )
    .await
    .expect_err("no refresh token");
    assert!(
        err.to_string().contains("refresh token"),
        "the refusal must name the refresh token: {err}",
    );
}

#[test]
fn the_pending_handle_round_trips_through_the_host() {
    // The host carries this opaque across a browser hop, so it must survive
    // serialisation exactly: a dropped verifier or state would fail the
    // exchange with a message that points nowhere near the cause.
    let pending = PendingJmapLogin {
        email: "alice@example.com".to_owned(),
        base_url: "https://api.example.com".to_owned(),
        client_id: "client-abc".to_owned(),
        client_secret: None,
        authorize_endpoint: "https://api.example.com/oauth/authorize".to_owned(),
        token_endpoint: "https://api.example.com/oauth/refresh".to_owned(),
        redirect_uri: REDIRECT_URI.to_owned(),
        scopes: vec!["offline_access".to_owned()],
        resource: Some("https://api.example.com/jmap/session".to_owned()),
        issuer: None,
        state: "state-xyz".to_owned(),
        verifier: "verifier".to_owned(),
    };
    let encoded = serde_json::to_string(&pending).expect("serialisable");
    let decoded: PendingJmapLogin = serde_json::from_str(&encoded).expect("decodable");
    assert_eq!(decoded.state, "state-xyz");
    assert_eq!(decoded.verifier, "verifier");
    assert_eq!(decoded.redirect_uri, REDIRECT_URI);
    assert_eq!(
        decoded.resource.as_deref(),
        Some("https://api.example.com/jmap/session"),
    );
}

/// A host sink that records what it was told, standing in for the desktop's
/// credential transaction.
#[derive(Default)]
struct RecordingSink(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl TokenSink for RecordingSink {
    async fn refresh_token_rotated(&self, _account: &AccountId, new_refresh_token: &str) {
        self.0
            .lock()
            .expect("sink lock")
            .push(new_refresh_token.to_owned());
    }

    async fn scopes_granted(&self, _account: &AccountId, _granted: &astra_oauth::GrantedScopes) {}
}

#[tokio::test]
async fn a_rotated_refresh_token_reaches_the_host_sink() {
    // The FASTMAIL ratchet rule: a ratcheting server revokes the grant over
    // a replay, and a rotation nobody stored kills the account at the next
    // launch. The JMAP leg hands its sink to the shared source, so the
    // rotation write is the host's to persist, never dropped.
    let (endpoint, _hits) = ratcheting_token_endpoint("rt-old");
    let section = stored_oauth_account("alice@example.com", "https://api.example.com");
    // The stored grant's token endpoint is swapped for the ratcheting one:
    // the source refreshes where the test points it.
    let sink = Arc::new(RecordingSink::default());
    let source = jmap_token_source_at(
        &section,
        endpoint,
        AccountId::try_from("alice@example.com@jmap:api.example.com").unwrap(),
        Some(Arc::clone(&sink) as Arc<dyn TokenSink>),
    )
    .expect("a source over the stored grant");
    let _ = source.access_token().await.expect("a minted token");
    let rotations = sink.0.lock().expect("sink lock");
    assert_eq!(
        rotations.as_slice(),
        ["rotated-1"],
        "the rotation never reached the host",
    );
}

#[tokio::test]
async fn a_section_without_a_grant_names_the_missing_credential() {
    let doc = "[jmap]\nemail = \"me@example.net\"\nbase_url = \"https://api.example.net\"\npassword = \"pw\"\n";
    let section = load_str(doc)
        .expect("a valid jmap document")
        .jmap
        .expect("the jmap section");
    let err = jmap_token_source(
        &section,
        AccountId::try_from("me@example.net@jmap:api.example.net").unwrap(),
        None,
        CredentialOrigin::Stored,
    )
    .expect_err("no stored grant");
    assert!(
        matches!(err, AccountError::Jmap(ref said) if said.contains("oauth")),
        "not said in terms of the oauth block: {err}",
    );
}

#[test]
fn no_debug_form_leaks_a_secret_of_the_leg() {
    let section: JmapAccount = stored_oauth_account("alice@example.com", "https://api.example.com");
    assert!(!format!("{section:?}").contains("rt-old"));
}

/// The public [`jmap_token_source`](super::jmap_token_source) over a token
/// endpoint the test chooses: the stored grant's endpoints, with the token
/// endpoint pointed at a mock.
fn jmap_token_source_at(
    section: &JmapAccount,
    token_endpoint: String,
    account: AccountId,
    sink: Option<Arc<dyn TokenSink>>,
) -> Result<Arc<crate::GraphTokenSource>, AccountError> {
    let mut grant = section.oauth.clone().expect("a stored grant");
    grant.token_endpoint = token_endpoint;
    let section = JmapAccount {
        oauth: Some(grant),
        ..section.clone()
    };
    jmap_token_source(&section, account, sink, CredentialOrigin::Stored)
}
