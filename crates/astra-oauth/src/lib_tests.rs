// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the begin/complete surface: the authorization URL's shape and
//! the redirect callback's validation (state, `iss`, and the provider's
//! own error round trip).

use super::*;

#[test]
fn begin_produces_a_url_that_embeds_the_generated_state_and_challenge() {
    let provider = OAuthProviderConfig::microsoft(
        "client-abc",
        "common",
        "com.kenizhou.astra://oauth",
        MICROSOFT_GRAPH_SCOPES,
    );
    let client = OAuthClient::new(provider).unwrap();
    let req = client.begin(None);

    let parsed = url::Url::parse(&req.authorization_url).unwrap();
    let q: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
    assert_eq!(q["state"], req.state);
    assert_eq!(q["code_challenge"], req.pkce.challenge());
}

#[test]
fn parse_callback_extracts_the_code_from_a_custom_scheme_redirect() {
    let code = parse_callback(
        "state-xyz",
        None,
        "com.kenizhou.astra://oauth?code=the-code&state=state-xyz",
    )
    .unwrap();
    assert_eq!(code, "the-code");
}

#[test]
fn parse_callback_handles_the_msauth_dotted_scheme_azure_generates() {
    // Azure auto-generates `msauth.<bundle-id>://auth` for a macOS/iOS
    // platform (the developer can't change it). The scheme contains dots;
    // make sure the parser still extracts code/state from it, not just the
    // simpler `scheme://host` form.
    let code = parse_callback(
        "state-xyz",
        None,
        "msauth.com.kenizhou.astra://auth?code=the-code&state=state-xyz",
    )
    .unwrap();
    assert_eq!(code, "the-code");
}

#[test]
fn parse_callback_extracts_the_code_from_a_bare_loopback_url() {
    // The Google Desktop flow's redirect is a plain http loopback URL a
    // host-side listener reads off the wire, not an OS-delivered scheme;
    // the parser reads the query wherever the URL landed.
    let code = parse_callback(
        "state-xyz",
        None,
        "http://127.0.0.1:51341/?code=the-code&state=state-xyz",
    )
    .unwrap();
    assert_eq!(code, "the-code");
}

#[test]
fn parse_callback_rejects_a_state_mismatch() {
    let err = parse_callback(
        "expected",
        None,
        "com.kenizhou.astra://oauth?code=c&state=attacker",
    )
    .unwrap_err();
    assert!(matches!(err, OAuthError::Callback(_)));
}

#[test]
fn parse_callback_rejects_a_missing_state() {
    let err = parse_callback("expected", None, "com.kenizhou.astra://oauth?code=c").unwrap_err();
    assert!(matches!(err, OAuthError::Callback(msg) if msg.contains("state")));
}

#[test]
fn parse_callback_surfaces_a_provider_error_in_the_redirect() {
    // The user cancelled or consent was denied, so the provider redirects
    // with `error`.
    let err = parse_callback(
        "state-xyz",
        None,
        "com.kenizhou.astra://oauth?error=access_denied&error_description=user%20cancelled&state=state-xyz",
    )
    .unwrap_err();
    match err {
        OAuthError::Endpoint { error, description } => {
            assert_eq!(error, "access_denied");
            assert_eq!(description.as_deref(), Some("user cancelled"));
        }
        other => panic!("expected Endpoint, got {other:?}"),
    }
}

#[test]
fn parse_callback_rejects_a_missing_code() {
    let err = parse_callback("s", None, "com.kenizhou.astra://oauth?state=s").unwrap_err();
    assert!(matches!(err, OAuthError::Callback(msg) if msg.contains("code")));
}

#[test]
fn a_matching_iss_passes_and_a_trailing_slash_is_not_a_difference() {
    // RFC 9207: the happy path, and the one textual difference an issuer
    // identifier is allowed to have. Rejecting on a slash would break real
    // servers for no security gain.
    let code = parse_callback(
        "s",
        Some("https://as.example.com"),
        "com.kenizhou.astra://oauth?code=c&state=s&iss=https%3A%2F%2Fas.example.com%2F",
    )
    .unwrap();
    assert_eq!(code, "c");
}

#[test]
fn an_iss_naming_another_server_aborts_before_the_code_is_sent_anywhere() {
    // The mix-up attack: an honest server's code arrives at the client
    // after a malicious authorization server relayed the request. `state`
    // matches (the attacker saw the request and could replay it), so `iss`
    // is the only thing that catches this, and the code must never reach
    // a token endpoint.
    let err = parse_callback(
        "s",
        Some("https://honest.example.com"),
        "com.kenizhou.astra://oauth?code=c&state=s&iss=https%3A%2F%2Fattacker.example.net",
    )
    .unwrap_err();
    assert!(matches!(err, OAuthError::Callback(msg) if msg.contains("attacker.example.net")));
}

#[test]
fn a_missing_iss_is_a_failure_when_the_server_said_it_sends_one() {
    // "Absent" must not read as "nothing to verify": deleting a query
    // parameter would otherwise be all it takes to switch the check off.
    let err = parse_callback(
        "s",
        Some("https://as.example.com"),
        "com.kenizhou.astra://oauth?code=c&state=s",
    )
    .unwrap_err();
    assert!(matches!(err, OAuthError::Callback(msg) if msg.contains("iss")));
}

#[test]
fn no_expected_issuer_ignores_whatever_iss_arrives() {
    // A server that never advertised the parameter gives us nothing to
    // compare against, so an `iss` that turns up anyway is not evidence of
    // anything and changes no decision.
    let code = parse_callback(
        "s",
        None,
        "com.kenizhou.astra://oauth?code=c&state=s&iss=https%3A%2F%2Fanything.example.net",
    )
    .unwrap();
    assert_eq!(code, "c");
}
