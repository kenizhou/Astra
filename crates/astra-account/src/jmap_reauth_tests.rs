// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the re-authentication: it begins from the persisted grant
//! with no network at all, it is refused for a password account (which has
//! no browser flow to re-run), and it swaps the grant in only when the
//! browser signed into the same mailbox.

use super::super::PendingJmapLogin;
use super::{begin_jmap_reauth, complete_jmap_reauth};
use crate::jmap::oauth::test_support::{
    discovery_server, full_chain, now, stored_oauth_account, REDIRECT_URI,
};
use crate::{load_str, JmapAccount};

#[test]
fn reauth_begins_from_the_persisted_grant_with_no_network() {
    // The endpoints and the client id the grant already carries are the
    // ones re-used; re-running RFC 7591 would mint a second client id on
    // the person's account, so begin makes no requests at all: it succeeds
    // against endpoints nothing answers.
    let section = stored_oauth_account("alice@example.com", "https://api.example.com");
    let start = begin_jmap_reauth(&section).expect("a re-auth built from the stored grant");
    assert!(
        start
            .authorization_url
            .starts_with("https://api.example.com/oauth/authorize?"),
        "{}",
        start.authorization_url,
    );
    assert!(
        start.authorization_url.contains("client_id=client-abc"),
        "{}",
        start.authorization_url,
    );
    let pending: PendingJmapLogin =
        serde_json::from_str(&start.pending).expect("a decodable handle");
    assert_eq!(pending.client_id, "client-abc");
    assert_eq!(pending.redirect_uri, REDIRECT_URI);
    assert_eq!(
        pending.resource.as_deref(),
        Some("https://api.example.com/jmap/session"),
    );
}

#[test]
fn reauth_of_a_password_account_names_its_limit() {
    // A password account has no browser flow to re-run: its remedy is a
    // fresh secret in Settings, and the message must say so.
    let doc = "[jmap]\nemail = \"me@example.net\"\nbase_url = \"https://api.example.net\"\npassword = \"pw\"\n";
    let section = load_str(doc)
        .expect("a valid jmap document")
        .jmap
        .expect("the jmap section");
    let err = begin_jmap_reauth(&section).expect_err("no stored grant");
    assert!(
        err.to_string().to_lowercase().contains("password"),
        "the refusal must name the password form: {err}",
    );
}

#[tokio::test]
async fn reauth_swaps_the_grant_only_for_the_same_mailbox() {
    let (base, _seen) = discovery_server(full_chain);
    // A sign-in for an address at the same server the stored section
    // points at: the same mailbox by the id both sides derive.
    let mut pending = PendingJmapLogin {
        email: "alice@example.net".to_owned(),
        base_url: base.clone(),
        client_id: "client-xyz".to_owned(),
        client_secret: None,
        authorize_endpoint: format!("{base}/oauth/authorize"),
        token_endpoint: format!("{base}/token"),
        redirect_uri: REDIRECT_URI.to_owned(),
        scopes: vec!["offline_access".to_owned()],
        resource: Some(format!("{base}/jmap/session")),
        issuer: Some(base.clone()),
        state: "state-1".to_owned(),
        verifier: "verifier".to_owned(),
    };
    let stored: JmapAccount = stored_oauth_account("alice@example.net", &base);
    let encoded = serde_json::to_string(&pending).expect("serialisable");
    let iss = format!("&iss={}", base.replace(':', "%3A").replace('/', "%2F"));
    let fresh = complete_jmap_reauth(
        &stored,
        &encoded,
        &format!("{REDIRECT_URI}?code=c1&state=state-1{iss}"),
        now(),
    )
    .await
    .expect("the same mailbox");
    assert_eq!(fresh.email, "alice@example.net");
    let grant = fresh.oauth.as_ref().expect("the fresh grant");
    assert_eq!(grant.refresh_token.expose(), "rt-1");
    assert_eq!(grant.token_endpoint, format!("{base}/token"));

    // A sign-in as a different mailbox is refused: writing that grant under
    // the account's id would point one person's config at another's mail
    // while still displaying the original address.
    pending.email = "bob@example.net".to_owned();
    let encoded = serde_json::to_string(&pending).expect("serialisable");
    let err = complete_jmap_reauth(
        &stored,
        &encoded,
        &format!("{REDIRECT_URI}?code=c2&state=state-1{iss}"),
        now(),
    )
    .await
    .expect_err("a different mailbox");
    assert!(
        err.to_string().contains("different account"),
        "the refusal must say which way it went wrong: {err}",
    );
}
