// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the token-bearing credential source: every dial is answered
//! from the shared token source, a refused token is renewed once through
//! the same single-flight, and a dead grant fails as an authentication
//! refusal rather than an outage.

use std::sync::Arc;

use engine_api::AccountId;
use engine_core::error::FailureClass;
use provider_imap::{CredentialSource, Credentials};

use super::*;
use crate::graph::token_source::test_support::{
    dead_grant_token_endpoint, ratcheting_token_endpoint,
};
use crate::{load_str, AccountError, CredentialOrigin, ImapAccount};

/// A grant-carrying `[imap]` section whose token endpoint the test owns.
fn grant_section(token_endpoint: &str) -> ImapAccount {
    load_str(&format!(
        "[imap]\naddr = \"127.0.0.1:993\"\nserver_name = \"127.0.0.1\"\n\
         username = \"you@example.net\"\n\
         \n[imap.oauth]\nclient_id = \"client-abc\"\nrefresh_token = \"seed\"\n\
         authorize_endpoint = \"http://127.0.0.1:2080/oauth/authorize\"\n\
         token_endpoint = \"{token_endpoint}\"\n\
         redirect_uri = \"com.kenizhou.astra://mail-oauth\"\n\
         scopes = [\"offline_access\"]\n",
    ))
    .expect("a valid grant document")
    .imap
    .expect("the imap section")
}

/// A distinct account for every test source: token state is shared per
/// account across a process, so one id for every test would couple them
/// through the ratcheting mock's memory of what it already served.
fn account_id() -> AccountId {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    AccountId::try_from(format!("you+{n}@example.net@imap.example.net").as_str())
        .expect("account id")
}

/// The OAuth credential the source hands a dial, as the engine sees it.
async fn oauth_token(source: &dyn CredentialSource) -> String {
    match source.credentials().await.expect("a credential") {
        Credentials::OAuth2 {
            username,
            access_token,
        } => {
            assert_eq!(username, "you@example.net");
            access_token
        }
        other => panic!("an OAuth source hands an OAuth credential: {other:?}"),
    }
}

#[tokio::test]
async fn every_dial_is_answered_with_a_token_valid_then() {
    let (endpoint, hits) = ratcheting_token_endpoint("seed");
    let section = grant_section(&endpoint);
    let tokens = imap_oauth_token_source(
        section.oauth.as_ref().expect("the grant"),
        account_id(),
        None,
        CredentialOrigin::Stored,
    )
    .expect("a token source over the grant's own endpoints");
    let source = imap_oauth_credential_source(&section, tokens).expect("a credential source");

    let first = oauth_token(source.as_ref()).await;
    assert_eq!(first, "AT-1");
    // A later dial is answered anew: the engine asks per connection, and
    // the cache only serves a token that is still comfortably valid.
    assert_eq!(oauth_token(source.as_ref()).await, "AT-1");
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_refused_token_renews_once_through_the_shared_source() {
    let (endpoint, hits) = ratcheting_token_endpoint("seed");
    let section = grant_section(&endpoint);
    let tokens = imap_oauth_token_source(
        section.oauth.as_ref().expect("the grant"),
        account_id(),
        None,
        CredentialOrigin::Stored,
    )
    .expect("a token source");
    let source = imap_oauth_credential_source(&section, tokens).expect("a credential source");

    let expired = Credentials::oauth2("you@example.net", oauth_token(source.as_ref()).await);
    // The server refused it; the engine asks the source once for a
    // replacement, which is one refresh through the shared single-flight.
    let renewed = source
        .renew(&expired)
        .await
        .expect("a renewal")
        .expect("a replacement token");
    let Credentials::OAuth2 {
        username,
        access_token,
    } = &renewed
    else {
        panic!("the renewal is an OAuth credential");
    };
    assert_eq!(username, "you@example.net");
    assert_eq!(access_token, "AT-2");
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 2);

    // The source never loops: a second refusal of the same spent token
    // finds the replacement cached, and no third refresh is posted.
    let again = source
        .renew(&expired)
        .await
        .expect("a renewal")
        .expect("the cached replacement");
    assert_eq!(
        match again {
            Credentials::OAuth2 { access_token, .. } => access_token,
            other => panic!("an OAuth credential: {other:?}"),
        },
        "AT-2"
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_refused_password_shape_is_not_renewed() {
    // A password is never this source's credential; a renewal asked for
    // one answers that there is nothing to present.
    let (endpoint, _hits) = ratcheting_token_endpoint("seed");
    let section = grant_section(&endpoint);
    let tokens = imap_oauth_token_source(
        section.oauth.as_ref().expect("the grant"),
        account_id(),
        None,
        CredentialOrigin::Stored,
    )
    .expect("a token source");
    let source = imap_oauth_credential_source(&section, tokens).expect("a credential source");
    let refused = Credentials::password("you@example.net", "not-ours");
    assert!(source.renew(&refused).await.expect("answered").is_none());
}

#[tokio::test]
async fn a_dead_grant_fails_with_the_authentication_class() {
    let (endpoint, _hits) = dead_grant_token_endpoint();
    let section = grant_section(&endpoint);
    let tokens = imap_oauth_token_source(
        section.oauth.as_ref().expect("the grant"),
        account_id(),
        None,
        CredentialOrigin::Stored,
    )
    .expect("a token source");
    let source = imap_oauth_credential_source(&section, tokens).expect("a credential source");
    let err = source
        .credentials()
        .await
        .expect_err("a dead grant mints nothing");
    assert_eq!(err.class(), FailureClass::Authentication);
}

#[test]
fn a_section_without_a_grant_is_refused_at_the_source() {
    let section = load_str(
        "[imap]\naddr = \"127.0.0.1:993\"\nserver_name = \"127.0.0.1\"\n\
         username = \"you@example.net\"\npassword = \"hunter2\"\n",
    )
    .expect("a password account")
    .imap
    .expect("the imap section");
    assert!(matches!(
        imap_oauth_credential_source(&section, dummy_tokens()),
        Err(AccountError::MissingCredential(_))
    ));
}

#[tokio::test]
async fn a_grant_account_builds_its_engine_config_over_the_oauth_source() {
    // The seam composes: a grant-carrying config, no password anywhere,
    // still builds the engine dial over the oauth credential source.
    let (endpoint, _hits) = ratcheting_token_endpoint("seed");
    let config = load_str(&format!(
        "[imap]\naddr = \"127.0.0.1:993\"\nserver_name = \"127.0.0.1\"\n\
         username = \"you@example.net\"\n\
         \n[imap.oauth]\nclient_id = \"client-abc\"\nrefresh_token = \"seed\"\n\
         authorize_endpoint = \"http://127.0.0.1:2080/oauth/authorize\"\n\
         token_endpoint = \"{endpoint}\"\n\
         redirect_uri = \"com.kenizhou.astra://mail-oauth\"\n\
         scopes = [\"offline_access\"]\n\
         \n[smtp]\naddr = \"127.0.0.1:465\"\nserver_name = \"127.0.0.1\"\n",
    ))
    .expect("a valid grant document");
    let section = config.imap.as_ref().expect("the imap section");
    let tokens = imap_oauth_token_source(
        section.oauth.as_ref().expect("the grant"),
        account_id(),
        None,
        CredentialOrigin::Stored,
    )
    .expect("a token source");
    let source = imap_oauth_credential_source(section, tokens).expect("a credential source");
    config.imap_config(source).expect("the engine dial builds");
    // And the password path refuses the same account: it has no password.
    assert!(matches!(
        crate::imap_credential_source(&config),
        Err(AccountError::MissingCredential(_))
    ));
}

/// A token source the refusal test can name without a network round
/// trip: its endpoint is never reached, because the constructor refuses
/// the section first.
fn dummy_tokens() -> Arc<crate::GraphTokenSource> {
    let (endpoint, _hits) = dead_grant_token_endpoint();
    imap_oauth_token_source(
        grant_section(&endpoint).oauth.as_ref().expect("the grant"),
        account_id(),
        None,
        CredentialOrigin::Stored,
    )
    .expect("a token source")
}
