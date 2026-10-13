// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the scope selection and the registration shapes: what a
//! newly registered client asks for, and what the fixed parts of its
//! registration document are.

use super::*;

fn metadata(scopes: &[&str]) -> AuthServerMetadata {
    AuthServerMetadata {
        issuer: "https://as.example.com".to_owned(),
        authorization_endpoint: "https://as.example.com/authorize".to_owned(),
        token_endpoint: "https://as.example.com/token".to_owned(),
        registration_endpoint: Some("https://as.example.com/register".to_owned()),
        revocation_endpoint: None,
        userinfo_endpoint: None,
        scopes_supported: scopes.iter().map(|s| (*s).to_owned()).collect(),
        end_session_endpoint: None,
        prompt_values_supported: Vec::new(),
        issuer_parameter_supported: false,
    }
}

#[test]
fn the_software_id_is_a_fixed_uuid() {
    // It identifies the *software*, not an install, so it must never be
    // generated per device or per run: a server keying on it would then
    // see every install as different software.
    assert_eq!(SOFTWARE_ID, "a3b661b6-e590-4386-82ad-f12d01bd533c");
    assert_eq!(SOFTWARE_ID.len(), 36);
}

#[test]
fn offline_access_is_always_requested() {
    // Without it the server issues no refresh token and the account breaks
    // an hour after setup: the single most expensive thing to get wrong
    // here.
    assert!(select_scopes(&metadata(&[])).contains(&OFFLINE_ACCESS.to_owned()));
    assert_eq!(
        select_scopes(&metadata(&["urn:ietf:params:oauth:scope:mail"]))[0],
        OFFLINE_ACCESS
    );
}

#[test]
fn only_the_capabilities_we_use_are_requested() {
    // The shape a real JMAP server advertises. Mail, calendars and
    // contacts are all exercised by the client; admin is not, and an
    // over-broad consent screen is a user-visible harm; we cannot honestly
    // ask for a permission we never use.
    let selected = select_scopes(&metadata(&[
        "urn:ietf:params:oauth:scope:mail",
        "urn:ietf:params:oauth:scope:contacts",
        "urn:ietf:params:oauth:scope:calendars",
        "https://example.com/auth/admin",
        "offline_access",
    ]));
    assert_eq!(
        selected,
        vec![
            "offline_access",
            "urn:ietf:params:oauth:scope:mail",
            "urn:ietf:params:oauth:scope:contacts",
            "urn:ietf:params:oauth:scope:calendars",
        ]
    );
    assert!(
        !selected.iter().any(|scope| scope.ends_with("admin")),
        "a capability the client never exercises must not reach the consent screen"
    );
}

#[test]
fn bare_and_path_style_scope_names_are_recognised_too() {
    // The match is on the last segment, so the URN form, a bare word and
    // a URL form all work: this is the "generic from the standards" rule,
    // not one provider's spelling.
    assert!(select_scopes(&metadata(&["mail"])).contains(&"mail".to_owned()));
    assert!(
        select_scopes(&metadata(&["https://example.com/auth/calendar"]))
            .contains(&"https://example.com/auth/calendar".to_owned())
    );
}

#[test]
fn offline_access_is_never_duplicated_when_the_server_advertises_it() {
    let selected = select_scopes(&metadata(&["offline_access", "mail"]));
    assert_eq!(
        selected.iter().filter(|s| *s == OFFLINE_ACCESS).count(),
        1,
        "a duplicated scope is rejected outright by some servers: {selected:?}"
    );
}

#[test]
fn a_server_offering_no_mail_scope_is_not_usable() {
    // A token good for calendars alone would surface much later as a
    // permanently empty mailbox. Catch it at discovery and fall back to
    // the manual secret instead.
    let meta = metadata(&["urn:ietf:params:oauth:scope:calendars"]);
    let selected = select_scopes(&meta);
    assert!(!grants_mail_access(&meta, &selected));

    let meta = metadata(&["urn:ietf:params:oauth:scope:mail"]);
    let selected = select_scopes(&meta);
    assert!(grants_mail_access(&meta, &selected));
}

#[test]
fn a_server_advertising_no_scopes_at_all_is_allowed_through() {
    // RFC 6749 section 3.3: with no scope advertised the server applies
    // its own default grant. That is legal and common on small self-hosted
    // servers, so it must not be mistaken for "no mail access".
    let meta = metadata(&[]);
    assert!(grants_mail_access(&meta, &select_scopes(&meta)));
}
