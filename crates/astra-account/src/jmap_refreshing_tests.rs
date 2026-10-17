// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the refreshing JMAP provider: the mint-before-call and the
//! delegate cache's rebuild discipline (reuse while the access token is
//! unchanged, rebuild exactly once when a refresh produced a new one), and
//! the boundary where a token-source refusal crosses into the engine's
//! failure taxonomy without a single JMAP request leaving the device.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use engine_api::{AccountId, FailureClass, Provider};
use engine_core::sync::JmapDataType;
use time::{Duration, OffsetDateTime};

use super::RefreshingJmapProvider;
use crate::graph::token_source::test_support::{
    dead_grant_token_endpoint, flaky_token_endpoint, mock_token_endpoint,
};
use crate::graph::GraphTokenSource;

/// A session document advertising mail for one account, the same shape a
/// healthy dial connects against.
fn session_body(port: u16) -> String {
    let mail = "urn:ietf:params:jmap:mail";
    format!(
        "{{\"apiUrl\":\"http://127.0.0.1:{port}/api\",\
         \"capabilities\":{{\"urn:ietf:params:jmap:core\":{{}},\"{mail}\":{{}}}},\
         \"primaryAccounts\":{{\"{mail}\":\"A1\"}},\
         \"accounts\":{{\"A1\":{{\"name\":\"scripted\",\"accountCapabilities\":{{\"{mail}\":{{}}}}}}}}}}",
    )
}

/// A scripted JMAP session server that counts every request it serves: the
/// count is how a test tells a reused delegate (no new session discovery)
/// from a rebuilt one (a fresh GET). Returns its base URL and the counter.
fn counting_session_server() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_thread = Arc::clone(&hits);
    std::thread::spawn(move || {
        let body = session_body(addr.port());
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            hits_thread.fetch_add(1, Ordering::SeqCst);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (format!("http://{addr}"), hits)
}

/// The TLS policy every offline test dials under.
fn tls() -> engine_tls::TlsClientConfig {
    engine_tls::client_config_with_exceptions(&crate::setup_trust_policy(), &[])
        .expect("a policy with no exceptions")
}

/// A **distinct** account per source, so one test's token state never
/// leaks into the next through the shared per-account credential.
fn unique_account() -> AccountId {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    AccountId::try_from(format!("alice+{n}@example.com@jmap:127.0.0.1").as_str())
        .expect("a valid account id")
}

/// A JMAP token source over a mock token endpoint: the shared
/// provider-neutral source wearing the `jmap` tag the leg passes it.
fn jmap_source_at(token_endpoint: String) -> Arc<GraphTokenSource> {
    use astra_oauth::{AuthStyle, OAuthClient, OAuthProviderConfig};
    let provider = OAuthProviderConfig {
        authorize_endpoint: "https://as.example.com/authorize".to_owned(),
        token_endpoint,
        client_id: "client-abc".to_owned(),
        client_secret: None,
        redirect_uri: "com.kenizhou.astra://oauth".to_owned(),
        scopes: vec!["offline_access".to_owned()],
        resource: None,
        expected_issuer: None,
        style: AuthStyle::Discovered,
    };
    GraphTokenSource::from_parts(
        OAuthClient::new(provider).expect("the test provider config builds"),
        unique_account(),
        "initial-refresh".to_owned(),
        None,
        "jmap",
        crate::CredentialOrigin::Stored,
    )
}

/// A wrapper whose first delegate is already connected against the
/// scripted session server, built with `access_token` and the session's
/// own capabilities: the state an OAuth dial hands the wrapper in.
async fn wrapper_over(
    base_url: &str,
    tokens: Arc<GraphTokenSource>,
    access_token: &str,
) -> RefreshingJmapProvider {
    let config = provider_jmap::JmapConfig::new(
        base_url.to_owned(),
        provider_jmap::Credentials::bearer(access_token.to_owned()),
    )
    .with_tls(tls())
    .with_retry(tokens.retry());
    let initial = provider_jmap::JmapProvider::connect(config)
        .await
        .expect("a connected initial delegate");
    let capabilities = initial.connection_info().capabilities;
    RefreshingJmapProvider::new(
        base_url.to_owned(),
        tokens,
        tls(),
        access_token.to_owned(),
        initial,
        capabilities,
    )
}

#[tokio::test]
async fn the_delegate_is_reused_while_the_access_token_is_unchanged() {
    let (base_url, session_hits) = counting_session_server();
    let (endpoint, _token_hits) = mock_token_endpoint(vec![
        r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT-1"}"#.to_owned(),
    ]);
    let tokens = jmap_source_at(endpoint);
    let wrapper = wrapper_over(&base_url, Arc::clone(&tokens), "AT-1").await;
    let initial_hits = session_hits.load(Ordering::SeqCst);

    // Two calls: the first mints (the cache holds nothing yet), the second
    // finds the minted token cached. Both match the delegate's token, so
    // neither re-runs session discovery: the delegate is reused, its
    // connection pool with it.
    let first = wrapper.delegate().await.expect("a delegate");
    let second = wrapper.delegate().await.expect("a delegate");
    assert!(Arc::ptr_eq(&first, &second), "the delegate was rebuilt");
    assert_eq!(
        session_hits.load(Ordering::SeqCst),
        initial_hits,
        "an unchanged token re-ran session discovery",
    );
}

#[tokio::test]
async fn a_refreshed_token_rebuilds_the_delegate_exactly_once() {
    let (base_url, session_hits) = counting_session_server();
    let (endpoint, _token_hits) = mock_token_endpoint(vec![
        r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT-1"}"#.to_owned(),
        r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT-2"}"#.to_owned(),
    ]);
    let tokens = jmap_source_at(endpoint);
    let wrapper = wrapper_over(&base_url, Arc::clone(&tokens), "AT-1").await;
    let initial_hits = session_hits.load(Ordering::SeqCst);
    let first = wrapper.delegate().await.expect("a delegate");

    // Expire the cached access token so the next mint refreshes, and the
    // refresh answers a different token: the one case that must rebuild.
    tokens.seed_access_token(
        "AT-1".to_owned(),
        OffsetDateTime::now_utc() - Duration::hours(2),
    );
    let rebuilt = wrapper.delegate().await.expect("a rebuilt delegate");
    assert!(
        !Arc::ptr_eq(&first, &rebuilt),
        "the stale delegate survived"
    );
    assert_eq!(
        session_hits.load(Ordering::SeqCst),
        initial_hits + 1,
        "the rebuild must re-run session discovery exactly once",
    );

    // And the rebuilt delegate is itself cached: the next call, on the same
    // token, costs no discovery.
    let again = wrapper.delegate().await.expect("the cached delegate");
    assert!(Arc::ptr_eq(&rebuilt, &again), "the rebuild was not cached");
    assert_eq!(session_hits.load(Ordering::SeqCst), initial_hits + 1);
}

#[tokio::test]
async fn the_wrapper_reports_the_initial_sessions_capabilities() {
    let (base_url, _session_hits) = counting_session_server();
    let (endpoint, _token_hits) = mock_token_endpoint(vec![
        r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT-1"}"#.to_owned(),
    ]);
    let wrapper = wrapper_over(&base_url, jmap_source_at(endpoint), "AT-1").await;
    // The scripted session advertises mail and nothing else, and the
    // wrapper reports what it saw rather than a guessed full set.
    let capabilities = Provider::connection_info(&wrapper).capabilities;
    assert!(capabilities.mail());
    assert!(!capabilities.calendars());
}

#[test]
fn the_scopes_are_the_account_wide_jmap_ones() {
    // The account-wide scopes every JMAP provider serves; the wrapper
    // forwards them explicitly so it stays honest if the delegate ever
    // narrows them.
    let account = AccountId::try_from("alice@example.com@jmap:mail.example.com").unwrap();
    let (base_url, _session_hits) = counting_session_server();
    let (endpoint, _token_hits) = mock_token_endpoint(vec![
        r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT-1"}"#.to_owned(),
    ]);
    let tokens = jmap_source_at(endpoint);
    let wrapper = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(wrapper_over(&base_url, tokens, "AT-1"));
    assert_eq!(
        wrapper.mailbox_scope(&account),
        engine_api::SyncScope::JmapType {
            account: account.clone(),
            data_type: JmapDataType::Mailbox,
        }
    );
    assert_eq!(
        wrapper.email_scope(&account),
        engine_api::SyncScope::JmapType {
            account: account.clone(),
            data_type: JmapDataType::Email,
        }
    );
    assert_eq!(
        wrapper.calendar_scope(&account),
        engine_api::SyncScope::JmapType {
            account: account.clone(),
            data_type: JmapDataType::Calendar,
        }
    );
    assert_eq!(
        wrapper.event_scope(&account),
        engine_api::SyncScope::JmapType {
            account,
            data_type: JmapDataType::CalendarEvent,
        }
    );
}

#[tokio::test]
async fn a_dead_grant_crosses_as_an_authentication_failure() {
    // The re-auth signal: the wrapper's caller must prompt "sign in again",
    // not badge an outage. Proven with no JMAP request leaving the device:
    // the refusal happens at the mint.
    let (base_url, session_hits) = counting_session_server();
    let (endpoint, _token_hits) = dead_grant_token_endpoint();
    let tokens = jmap_source_at(endpoint);
    let wrapper = wrapper_over(&base_url, Arc::clone(&tokens), "AT-1").await;
    tokens.seed_access_token(
        "AT-1".to_owned(),
        OffsetDateTime::now_utc() - Duration::hours(2),
    );
    let err = wrapper.delegate().await.expect_err("a dead grant");
    assert!(
        err.class() == FailureClass::Authentication,
        "a dead grant read as {err}",
    );
    assert_eq!(
        session_hits.load(Ordering::SeqCst),
        1,
        "a refused mint must not dial the JMAP server",
    );
}

#[tokio::test]
async fn a_transient_refresh_failure_crosses_as_retryable() {
    // A refresh that may have reached the server is held off by the source;
    // the wrapper's half of the contract is only that the failure crosses
    // as retryable rather than fatal.
    let (base_url, _session_hits) = counting_session_server();
    let (endpoint, _token_hits) = flaky_token_endpoint(1);
    let tokens = jmap_source_at(endpoint);
    let wrapper = wrapper_over(&base_url, tokens, "AT-1").await;
    let err = wrapper
        .delegate()
        .await
        .expect_err("a refresh with no answer");
    assert_eq!(err.class(), FailureClass::Retryable, "{err}");
}
