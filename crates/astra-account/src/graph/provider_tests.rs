// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the refreshing Graph mail provider: the capability set it
//! answers before any delegate exists, and the token boundary where a
//! token-source refusal crosses into the engine's failure taxonomy
//! without a single Graph request leaving the device.

use std::sync::atomic::Ordering;

use engine_api::{AccountId, FailureClass, MailboxId, Provider, SyncScope};
use engine_provider::ProviderError;

use super::should_reconnect;
use crate::graph::token_source::test_support::{
    dead_grant_token_endpoint, flaky_token_endpoint, mock_token_endpoint, source_at,
};

/// The account's TLS policy with no accepted exceptions: the shape every
/// offline provider test dials under.
fn tls() -> engine_tls::TlsClientConfig {
    engine_tls::client_config_with_exceptions(&crate::setup_trust_policy(), &[]).unwrap()
}

#[test]
fn a_retryable_transport_error_triggers_a_reconnect_but_a_rate_limit_or_auth_does_not() {
    // A retryable transport failure (a stale keep-alive socket after sleep) drops the
    // cached delegate and rebuilds a fresh client + connection pool...
    assert!(should_reconnect(&ProviderError::retryable("broken pipe")));
    // ...while a 429 is waited out on the *same* connection one layer down, not by a
    // reconnect, and an auth failure is never a transport reconnect (the host re-auths).
    assert!(!should_reconnect(&ProviderError::new(
        FailureClass::RateLimited,
        "429",
    )));
    assert!(!should_reconnect(&ProviderError::authentication("401")));
}

#[test]
fn a_folder_binding_answers_its_capabilities_before_any_delegate_exists() {
    let (endpoint, hits) = mock_token_endpoint(vec![]);
    let source = source_at(endpoint, None);
    let account = AccountId::try_from("alice@example.com@graph.microsoft.com").unwrap();
    let provider = crate::connect_graph_folder(source, "custom-folder-id", None, &tls()).unwrap();
    // Mail read + writes (mark-read/flag, move, delete) + submission (sendMail is
    // account-level, so any folder-bound provider advertises it), no calendar; all
    // reported before any delegate is built (no network yet), so a host can gate mail
    // actions and the composer's send up front. The per-folder scope names the bound
    // folder.
    assert!(provider.connection_info().capabilities.mail());
    assert!(provider.connection_info().capabilities.mail_writes());
    assert!(provider.connection_info().capabilities.submission());
    // And specifically that it can send an *iMIP* message: a `Draft` carrying a
    // `text/calendar` part with `method=`. Sending ordinary mail is a different
    // promise: an account that can do the first and not the second cannot answer an
    // invitation on a calendar server that does no scheduling, and the card has to
    // say so rather than offering three buttons that go nowhere.
    assert!(provider
        .connection_info()
        .capabilities
        .scheduling_submission());
    assert!(!provider.connection_info().capabilities.calendars());
    assert_eq!(
        provider.email_scope(&account),
        SyncScope::GraphFolder {
            account: account.clone(),
            folder: MailboxId::try_from("custom-folder-id").unwrap(),
        }
    );
    // Capabilities and scopes answer without minting: nothing has dialled, so the
    // token source was never asked.
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "answering capabilities minted a token",
    );
}

/// The token boundary: the wrapper asks the source before it dials, so a
/// dead grant surfaces where the host can act on it (a re-auth prompt,
/// never an outage badge), and no Graph request is attempted.
#[tokio::test]
async fn a_dead_grant_surfaces_as_an_authentication_refusal_before_any_graph_call() {
    let (endpoint, hits) = dead_grant_token_endpoint();
    let source = source_at(endpoint, None);
    let account = AccountId::try_from("alice@example.com@graph.microsoft.com").unwrap();
    let provider = crate::connect_graph_folder(source, "custom-folder-id", None, &tls()).unwrap();

    let err = provider.sync_mailboxes(&account, None).await.unwrap_err();
    assert_eq!(
        err.class(),
        FailureClass::Authentication,
        "a dead grant is a refused sign-in, not a server that cannot be reached: {err}",
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1, "the source was asked once");
}

/// A transient refresh failure keeps the retryable class (the caller backs
/// off and retries), and the wrapper's one reconnect takes the source's
/// shared failure memo rather than presenting the same refresh token a
/// second time.
#[tokio::test]
async fn a_transient_refresh_failure_stays_retryable_and_the_reconnect_never_reposts() {
    // Delivers every request, then hangs up without answering: the
    // maybe-processed shape, whose retry is the dangerous one.
    let (endpoint, hits) = flaky_token_endpoint(usize::MAX);
    let source = source_at(endpoint, None);
    let account = AccountId::try_from("alice@example.com@graph.microsoft.com").unwrap();
    let provider = crate::connect_graph_folder(source, "custom-folder-id", None, &tls()).unwrap();

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

#[test]
fn a_graph_only_document_derives_its_account_id_from_the_mailbox() {
    // A graph-only account has no IMAP section to key the id from: the
    // grant's mailbox address under the protocol it dials names it, so
    // the sign-in and the boot derive the same id from the same document.
    let document = "[graph]\nemail=\"person@outlook.com\"\nclient_id=\"cid\"\ntenant=\"ten\"\n\
                    redirect_uri=\"com.kenizhou.astra://oauth\"\nscopes=[\"a\", \"b\"]\n\
                    refresh_token=\"rt\"\n";
    let config = crate::config::load_str(document).expect("the graph-only document loads");
    let id = config.derived_account_id().expect("the id derives");
    assert_eq!(id.as_str(), "person@outlook.com@graph");
}

#[test]
fn the_refreshing_providers_debug_forms_carry_no_token() {
    // The cached delegate is keyed by the live access token; the Debug
    // forms name the binding and nothing else, so a logged Box<dyn
    // Provider> never hands out the credential.
    let (endpoint, _hits) = mock_token_endpoint(vec![]);
    let source = source_at(endpoint, None);
    source.seed_access_token(
        "live-secret-token".to_owned(),
        time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    );
    let provider = super::RefreshingGraphProvider::new(
        engine_api::MailboxId::try_from("inbox-folder-id").unwrap(),
        source,
        None,
        tls(),
    );
    let rendered = format!("{provider:?}");
    assert!(!rendered.contains("live-secret-token"));
}
