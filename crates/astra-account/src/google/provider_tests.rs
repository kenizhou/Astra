// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the refreshing Gmail provider: the capability set it
//! answers before any delegate exists, the account-global scopes, and
//! the token boundary where a token-source refusal crosses into the
//! engine's failure taxonomy without a single Google request leaving
//! the device.

use std::sync::atomic::Ordering;

use engine_api::{AccountId, FailureClass, IdentityControls, Provider, SyncScope};
use engine_provider::ProviderError;

use super::should_reconnect;
use crate::connect_google_folder;
use crate::google::test_support::google_source_at;
use crate::graph::token_source::test_support::{
    dead_grant_token_endpoint, flaky_token_endpoint, mock_token_endpoint,
};

/// The account's TLS policy with no accepted exceptions: the shape every
/// offline provider test dials under.
fn tls() -> engine_tls::TlsClientConfig {
    engine_tls::client_config_with_exceptions(&crate::setup_trust_policy(), &[]).unwrap()
}

#[test]
fn a_retryable_transport_error_reconnects_but_a_rate_limit_or_auth_does_not() {
    // A retryable transport failure (a stale keep-alive socket after
    // sleep) drops the cached delegate and rebuilds a fresh client plus
    // connection pool...
    assert!(should_reconnect(&ProviderError::retryable("broken pipe")));
    // ...while a 429 is waited out on the same connection one layer
    // down, not by a reconnect, and an auth failure is never a transport
    // reconnect (the host re-auths).
    assert!(!should_reconnect(&ProviderError::new(
        FailureClass::RateLimited,
        "429",
    )));
    assert!(!should_reconnect(&ProviderError::authentication("401")));
}

#[test]
fn connect_google_folder_binds_a_writable_account_global_mail_provider() {
    let (endpoint, hits) = mock_token_endpoint(vec![]);
    let source = google_source_at(endpoint, None, None);
    let account = AccountId::try_from("alice@gmail.com@google").unwrap();
    let provider = connect_google_folder(source, None, &tls());
    // Mail read/sync, and the account-global message scope (no folder
    // binding: Gmail's history cursor is account-wide).
    assert!(provider.connection_info().capabilities.mail());
    assert!(!provider.connection_info().capabilities.calendars());
    // Writes and submission are forwarded, so they must be advertised. A
    // capability this wrapper does not claim is never attempted by the
    // app, and one it claims but does not forward hits the trait's
    // rejecting default, which is how every Gmail archive, delete,
    // mark-read and send used to fail with "provider does not support
    // mail writes" even though the delegate implements all of them.
    assert!(provider.connection_info().capabilities.mail_writes());
    assert!(provider.connection_info().capabilities.submission());
    // Same rule, one capability further along: this wrapper reports its
    // own set and never the delegate's, so an iMIP capability missing
    // here is missing from the account no matter what Gmail advertises.
    assert!(provider
        .connection_info()
        .capabilities
        .scheduling_submission());
    // The Google delta from the Graph wrapper: Gmail's send-as settings
    // are the account holder's to change rather than a directory's to
    // hand down, so the name is writable.
    assert_eq!(
        provider.connection_info().capabilities.sender_identities(),
        Some(IdentityControls::Writable),
    );
    assert_eq!(
        provider.email_scope(&account),
        SyncScope::GmailMessages {
            account: account.clone(),
        }
    );
    assert_eq!(
        provider.mailbox_scope(&account),
        SyncScope::GmailLabelList { account },
    );
    // Capabilities and scopes answer without minting: nothing has
    // dialled, so the token source was never asked.
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "answering capabilities minted a token",
    );
}

/// The token boundary: the wrapper asks the source before it dials, so a
/// dead grant surfaces where the host can act on it (a re-auth prompt,
/// never an outage badge), and no Google request is attempted.
#[tokio::test]
async fn a_dead_grant_surfaces_as_an_authentication_refusal_before_any_google_call() {
    let (endpoint, hits) = dead_grant_token_endpoint();
    let source = google_source_at(endpoint, None, None);
    let account = AccountId::try_from("alice@gmail.com@google").unwrap();
    let provider = connect_google_folder(source, None, &tls());

    let err = provider.sync_mailboxes(&account, None).await.unwrap_err();
    assert_eq!(
        err.class(),
        FailureClass::Authentication,
        "a dead grant is a refused sign-in, not a server that cannot be reached: {err}",
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1, "the source was asked once");
}

/// A transient refresh failure keeps the retryable class (the caller
/// backs off and retries), and the wrapper's one reconnect takes the
/// source's shared failure memo rather than presenting the same refresh
/// token a second time.
#[tokio::test]
async fn a_transient_refresh_failure_stays_retryable_and_the_reconnect_never_reposts() {
    // Delivers every request, then hangs up without answering: the
    // maybe-processed shape, whose retry is the dangerous one.
    let (endpoint, hits) = flaky_token_endpoint(usize::MAX);
    let source = google_source_at(endpoint, None, None);
    let account = AccountId::try_from("alice@gmail.com@google").unwrap();
    let provider = connect_google_folder(source, None, &tls());

    let err = provider.sync_mailboxes(&account, None).await.unwrap_err();
    assert_eq!(
        err.class(),
        FailureClass::Retryable,
        "a transient refresh failure is an outage, not a refused sign-in: {err}",
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "the retry re-presented a refresh token the server may have spent",
    );
}

/// The mail connect probes the credential with one refresh, so a dead
/// grant is a connect failure the host can act on now, not a surprise
/// at the first background sync.
#[tokio::test]
async fn the_mail_connect_probes_the_credential_so_a_dead_grant_fails_the_connect() {
    let (endpoint, hits) = dead_grant_token_endpoint();
    let source = google_source_at(endpoint, None, None);

    let connected = crate::connect_google_mail_providers(source, None, &tls()).await;
    let Err(err) = connected else {
        panic!("a dead grant connected: the probe never asked the source")
    };
    assert!(
        matches!(err, crate::AccountError::SigninRejected(_)),
        "the connect reported an outage for a grant the server refused: {err}",
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "the connect presented a refresh token twice",
    );
}

#[test]
fn the_refreshing_providers_debug_forms_carry_no_token() {
    // The cached delegate is keyed by the live access token; the Debug
    // form names the wrapper and nothing else, so a logged
    // Box<dyn Provider> never hands out the credential.
    let (endpoint, _hits) = mock_token_endpoint(vec![]);
    let source = google_source_at(endpoint, None, None);
    source.seed_access_token(
        "live-secret-token".to_owned(),
        time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    );
    let provider = super::RefreshingGmailProvider::new(source, None, tls());
    let rendered = format!("{provider:?}");
    assert!(!rendered.contains("live-secret-token"));
}

#[test]
fn the_calendar_wrapper_s_debug_form_carries_no_token() {
    // The twin of the mail wrapper's pin: the cached delegate is keyed by
    // the live token; the calendar's Debug names the binding and nothing
    // else.
    let document = concat!(
        "[google]
",
        "email = \"ada@gl.com\"
",
        "client_id = \"google-client\"
",
        "redirect_uri = \"http://127.0.0.1:49152/oauth2redirect\"
",
        "scopes = [\"https://mail.google.com/\"]
",
        "refresh_token = \"live-refresh\"
",
    );
    let config = crate::load_str(document).expect("the google document loads");
    let account = engine_api::AccountId::try_from("ada@gl.com@google").expect("the account id");
    let source = crate::google::google_token_source(
        config.google.as_ref().expect("the google section"),
        account,
        None,
        crate::CredentialOrigin::Stored,
    )
    .expect("the source builds");
    source.seed_access_token(
        "live-secret-token".to_owned(),
        time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    );
    let calendar = crate::google::calendar::RefreshingGoogleCalendarProvider::new(
        engine_api::CalendarId::try_from("cal-1").unwrap(),
        source,
        crate::google::calendar::tests_window().unwrap(),
        tls(),
    );
    let rendered = format!("{calendar:?}");
    assert!(!rendered.contains("live-secret-token"));
}
