// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the `[imap.oauth]` grant: the stored round trip, the
//! no-password rule of the grant path, the fold a document with both
//! resolves through, and the provider config the grant refreshes
//! through.

use astra_oauth::AuthStyle;

use crate::load_str;

/// A grant block with every optional field set, at loopback endpoints
/// the offline tests own.
fn grant_block() -> String {
    "[imap.oauth]\n\
     client_id = \"client-abc\"\n\
     client_secret = \"not-confidential\"\n\
     refresh_token = \"rt-old\"\n\
     authorize_endpoint = \"http://127.0.0.1:2080/oauth/authorize\"\n\
     token_endpoint = \"http://127.0.0.1:2080/token\"\n\
     redirect_uri = \"com.kenizhou.astra://mail-oauth\"\n\
     scopes = [\"offline_access\", \"mail\"]\n\
     issuer = \"http://127.0.0.1:2080\"\n"
        .to_owned()
}

fn imap_section(extra: &str) -> String {
    format!(
        "[imap]\naddr = \"127.0.0.1:993\"\nserver_name = \"127.0.0.1\"\n\
         username = \"you@example.net\"\n{extra}",
    )
}

#[test]
fn an_oauth_grant_round_trips_beside_the_imap_section() {
    let config = load_str(&imap_section(&grant_block())).expect("a grant-carrying document loads");
    let stored = config.to_toml().expect("serialisable config");
    // The grant path stores no password: the stored form never carries one.
    assert!(!stored.contains("password"), "{stored}");
    let reloaded = load_str(&stored).expect("round-trips");
    let grant = reloaded
        .imap
        .as_ref()
        .and_then(|imap| imap.oauth.as_ref())
        .expect("the grant survives the round trip");
    assert_eq!(grant.client_id, "client-abc");
    assert_eq!(
        grant.client_secret.as_ref().expect("secret").expose(),
        "not-confidential"
    );
    assert_eq!(grant.refresh_token.expose(), "rt-old");
    assert_eq!(
        grant.authorize_endpoint,
        "http://127.0.0.1:2080/oauth/authorize"
    );
    assert_eq!(grant.token_endpoint, "http://127.0.0.1:2080/token");
    assert_eq!(grant.redirect_uri, "com.kenizhou.astra://mail-oauth");
    assert_eq!(grant.scopes, ["offline_access", "mail"]);
    assert_eq!(grant.issuer.as_deref(), Some("http://127.0.0.1:2080"));
    // A second hop is stable: the reloaded document stores back the same shape.
    let stored_again = reloaded.to_toml().expect("serialisable config");
    assert_eq!(stored, stored_again);
}

#[test]
fn a_document_with_both_resolves_to_the_grant_and_stores_no_password() {
    // A hand-edited document can carry a stale password beside a grant;
    // the grant is the credential the account was last connected with
    // (the JMAP precedence), and the no-password rule reasserts itself
    // at the storage boundary.
    let document = imap_section(&format!(
        "password = \"leftover\"\n\n{grant}",
        grant = grant_block()
    ));
    let config = load_str(&document).expect("loads");
    let imap = config.imap.as_ref().expect("imap");
    assert!(
        imap.password.is_none(),
        "a grant-carrying account holds no password"
    );
    assert!(imap.oauth.is_some());
    let stored = config.to_toml().expect("serialisable config");
    assert!(!stored.contains("leftover"), "{stored}");
    assert!(!stored.contains("password"), "{stored}");
}

#[test]
fn a_password_account_stores_back_without_the_oauth_key() {
    // Additive only: a document written before the section existed
    // stores back without a trace of it.
    let document = imap_section("password = \"hunter2\"\n");
    let config = load_str(&document).expect("loads");
    assert!(config.imap.as_ref().expect("imap").oauth.is_none());
    let stored = config.to_toml().expect("serialisable config");
    assert!(!stored.contains("oauth"), "{stored}");
    assert!(stored.contains("hunter2"));
}

#[test]
fn a_grant_without_the_optional_fields_still_loads() {
    // A document written before `client_secret` or `issuer` existed:
    // absent means nothing to send and nothing to compare.
    let older = "[imap.oauth]\n\
                  client_id = \"client-abc\"\n\
                  refresh_token = \"rt-old\"\n\
                  authorize_endpoint = \"https://id.example.net/oauth/authorize\"\n\
                  token_endpoint = \"https://id.example.net/token\"\n\
                  redirect_uri = \"com.kenizhou.astra://mail-oauth\"\n\
                  scopes = [\"offline_access\"]\n";
    let config = load_str(&imap_section(older)).expect("loads");
    let grant = config
        .imap
        .as_ref()
        .and_then(|imap| imap.oauth.as_ref())
        .expect("the grant");
    assert!(grant.client_secret.is_none());
    assert!(grant.issuer.is_none());
}

#[test]
fn the_grant_maps_to_the_discovered_provider_config() {
    let config = load_str(&imap_section(&grant_block())).expect("a grant-carrying document loads");
    let grant = config
        .imap
        .as_ref()
        .and_then(|imap| imap.oauth.as_ref())
        .expect("the grant");
    let provider = grant.provider_config();
    assert_eq!(provider.authorize_endpoint, grant.authorize_endpoint);
    assert_eq!(provider.token_endpoint, grant.token_endpoint);
    assert_eq!(provider.client_id, grant.client_id);
    assert_eq!(provider.client_secret.as_deref(), Some("not-confidential"));
    assert_eq!(provider.redirect_uri, grant.redirect_uri);
    assert_eq!(provider.scopes, grant.scopes);
    assert_eq!(provider.expected_issuer, grant.issuer);
    assert_eq!(provider.style, AuthStyle::Discovered);
    // The RFC 8707 resource is the JMAP leg's; a mail grant never sends one.
    assert!(provider.resource.is_none());
}

#[test]
fn the_grant_debug_output_redacts_its_secrets() {
    let config = load_str(&imap_section(&grant_block())).expect("a grant-carrying document loads");
    let dump = format!("{config:?}");
    assert!(!dump.contains("rt-old"), "{dump}");
    assert!(!dump.contains("not-confidential"), "{dump}");
}
