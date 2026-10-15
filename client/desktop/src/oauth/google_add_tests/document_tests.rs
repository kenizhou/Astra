// SPDX-License-Identifier: GPL-3.0-only
//! The document's shape from the token set: the granted set an empty
//! response scope stands for, the refusal a grant without a refresh token
//! meets, and the dead word a refused refresh during the dial reads as.

use super::*;

#[test]
fn an_empty_response_scope_grants_the_whole_ask() {
    let host = Host::new("google-add-full-grant");
    let mut tokens = token_set();
    tokens.scope = String::new();

    let wiring = host.wiring();
    let dial = ok_dial(Arc::new(DialSaw::default()));
    let source = production_google_source();
    let lookup = lookup_seam("alice@example.test");
    let provider = provider_config();
    host.runtime
        .block_on(move || complete_google_signin(wiring, tokens, provider, lookup, dial, source))
        .expect("the sign-in completes");

    let google = stored_google(&host, &google_id("alice@example.test"));
    assert_eq!(
        google.granted_scopes,
        Some(provider_config().scopes),
        "an empty scope means granted as asked"
    );
}

#[test]
fn a_sign_in_without_a_refresh_token_refuses() {
    let host = Host::new("google-add-no-refresh");
    let mut tokens = token_set();
    tokens.refresh_token = None;

    let wiring = host.wiring();
    let dial = ok_dial(Arc::new(DialSaw::default()));
    let source = production_google_source();
    let lookup = lookup_seam("alice@example.test");
    let provider = provider_config();
    let failure = host
        .runtime
        .block_on(move || complete_google_signin(wiring, tokens, provider, lookup, dial, source))
        .expect_err("a grant without a refresh token breaks in an hour");

    assert!(
        failure.detail.contains("refresh token"),
        "the refusal names what is missing: {}",
        failure.detail
    );
    assert!(
        host.dials.ids().is_empty(),
        "no account was registered over the unusable grant"
    );
}

#[test]
fn a_dead_grant_during_the_dial_classifies_as_dead() {
    let host = Host::new("google-add-dead-grant");
    let dial = dial_seam(Arc::new(DialSaw::default()), |_| async move {
        Err(AccountError::SigninRejected(
            "oauth endpoint error: invalid_grant".to_owned(),
        ))
    });

    let failure = complete(
        &host,
        lookup_seam("alice@example.test"),
        dial,
        production_google_source(),
    )
    .expect_err("the dead grant refuses");

    assert_eq!(
        failure.refusal,
        RefusalKind::Dead,
        "a refused refresh is the dead word, not an outage"
    );
}
