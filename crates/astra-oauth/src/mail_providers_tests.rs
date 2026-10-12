// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the known-provider table's matching: which hosts an entry
//! covers, and the label boundary that keeps one provider's hosts out of
//! another's sign-in.

use super::*;

/// A stand-in for the entry a real provider gets, so the matching is
/// exercised on a build that ships none. The reserved domain is
/// deliberate: this must never be able to name a host that resolves.
fn table() -> Vec<StaticMailProvider> {
    vec![StaticMailProvider {
        label: "Example Mail",
        hosts: &["mail.example.net", "example.org"],
        issuer: "https://login.example.net",
        authorize_endpoint: "https://login.example.net/authorize",
        token_endpoint: "https://login.example.net/token",
        scopes: &["mail-r", "mail-w"],
        client_id: "static-client".to_owned(),
        client_secret: None,
    }]
}

#[test]
fn a_build_that_injected_nothing_offers_nothing() {
    // The shipped state, asserted rather than assumed: an empty table must
    // not be a half-configured one that offers a route with no client id
    // behind it.
    assert!(static_mail_providers().is_empty());
}

#[test]
fn a_host_and_its_subdomains_match_their_entry() {
    let table = table();
    for host in ["mail.example.net", "imap.mail.example.net", "example.org"] {
        assert!(
            provider_for_host(&table, host).is_some(),
            "{host} should match"
        );
    }
}

#[test]
fn matching_stops_at_a_label_boundary() {
    // `notexample.org` ends with `example.org` as text and belongs to
    // somebody else. Sending mail credentials to the wrong provider's
    // sign-in page is what this stops.
    let table = table();
    for host in ["notexample.org", "example.net", "example.org.example.com"] {
        assert!(
            provider_for_host(&table, host).is_none(),
            "{host} must not match"
        );
    }
}

#[test]
fn a_host_is_matched_without_regard_to_case_or_a_trailing_dot() {
    // A hostname read off a DNS answer is fully qualified and a user
    // types any case.
    let table = table();
    assert!(provider_for_host(&table, "IMAP.Mail.Example.NET.").is_some());
}
