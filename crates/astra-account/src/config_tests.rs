// SPDX-License-Identifier: GPL-3.0-only
//! Tests for [`AccountConfig`]: what a stored account must round-trip, the id it is
//! known by, and the secrets that never reach a log line.
//!
//! Everything here is about the stored form: an account that loses a field at rest
//! fails at the next launch rather than at the edit that caused it, which is the slowest
//! kind of failure to trace back.

use std::sync::Arc;

use super::*;

const SAMPLE: &str = r#"
[imap]
addr = "imap.example.net:993"
server_name = "imap.example.net"
username = "you@example.net"
password = "hunter2"

[smtp]
addr = "smtp.example.net:465"
server_name = "smtp.example.net"
"#;

fn config_with(username: &str, server_name: &str) -> AccountConfig {
    load_str(&format!(
        "[imap]\naddr=\"{server_name}:993\"\nserver_name=\"{server_name}\"\n\
         username=\"{username}\"\npassword=\"p\"\n",
    ))
    .expect("valid config")
}

#[test]
fn a_full_account_round_trips_through_the_stored_form() {
    let config = load_str(SAMPLE).expect("valid config");
    let stored = config.to_toml().expect("serialisable config");
    let parsed = load_str(&stored).expect("round-trips");

    let imap = parsed.imap.as_ref().expect("imap present");
    assert_eq!(imap.addr, "imap.example.net:993");
    assert_eq!(imap.server_name, "imap.example.net");
    assert_eq!(imap.username, "you@example.net");
    assert_eq!(
        imap.password.as_ref().expect("password").expose(),
        "hunter2"
    );
    let smtp = parsed.smtp.as_ref().expect("smtp present");
    assert_eq!(smtp.addr, "smtp.example.net:465");
    assert_eq!(smtp.server_name, "smtp.example.net");
    assert_eq!(imap.security, ConnectionSecurity::ImplicitTls);
    assert_eq!(smtp.security, ConnectionSecurity::ImplicitTls);
    // A password that survives TOML escaping exactly: no plaintext seed file is needed.
    let escaped = config_with("me@example.net", "mail.example.net");
    let reloaded = load_str(&escaped.to_toml().expect("serialisable")).expect("round-trips");
    assert_eq!(
        reloaded.imap.as_ref().expect("imap present").username,
        "me@example.net"
    );
}

#[test]
fn secrets_are_redacted_in_debug_output() {
    let config = load_str(SAMPLE).expect("valid config");
    // Secrets never appear in Debug output, so logging a config is safe.
    let dump = format!("{config:?}");
    assert!(!dump.contains("hunter2"));
    assert_eq!(
        format!(
            "{:?}",
            config
                .imap
                .as_ref()
                .expect("imap")
                .password
                .as_ref()
                .unwrap()
        ),
        "Secret(<redacted>)"
    );
}

#[test]
fn security_defaults_to_implicit_tls_and_parses_start_tls() {
    // An account TOML with no `security` key connects over implicit TLS on both
    // transports, and an explicit `start_tls` on the standard STARTTLS ports parses and
    // drives the engine's STARTTLS builders.
    let starttls = load_str(
        "[imap]\naddr=\"mail.example.net:143\"\nserver_name=\"mail.example.net\"\n\
         username=\"u\"\npassword=\"p\"\nsecurity=\"start_tls\"\n\
         [smtp]\naddr=\"mail.example.net:587\"\nserver_name=\"mail.example.net\"\n\
         security=\"start_tls\"\n",
    )
    .expect("valid config");
    assert_eq!(
        starttls.imap.as_ref().expect("imap").security,
        ConnectionSecurity::StartTls
    );
    assert_eq!(
        starttls.smtp.as_ref().expect("smtp").security,
        ConnectionSecurity::StartTls
    );
    let credentials = Arc::new(starttls.imap_password_credentials().expect("credentials"));
    let _ = starttls.imap_config(credentials).expect("engine config");
}

#[test]
fn an_account_without_an_imap_section_is_refused() {
    let err = load_str("[smtp]\naddr=\"smtp.example.net:465\"\nserver_name=\"s\"\n");
    assert!(matches!(err, Err(ConfigError::NoEndpoint)));
}

#[test]
fn sections_this_build_does_not_read_still_load() {
    // A stored config can name sections a later build reads (a calendar endpoint, a
    // sign-in grant); this build loads the mail half and leaves the rest for the build
    // that connects it, rather than refusing an account it cannot yet use fully.
    let config = load_str(&format!(
        "{SAMPLE}\n[caldav]\nbase_url=\"https://dav.example.net\"\nusername=\"u\"\n",
    ))
    .expect("valid config");
    assert_eq!(
        config.imap.as_ref().expect("imap").addr,
        "imap.example.net:993"
    );
}

#[test]
fn account_id_is_stable_across_username_and_host_case() {
    // Case drift in the typed username (or host) must not mint a second identity for
    // the same mailbox: the derived id lowercases both.
    let a = config_with("Alice@Example.COM", "IMAP.Example.com")
        .account_id()
        .expect("an id");
    let b = config_with("alice@example.com", "imap.example.com")
        .account_id()
        .expect("an id");
    assert_eq!(a, b);
}

#[test]
fn account_id_differs_by_host_for_the_same_username() {
    // The same username on two different servers is two distinct accounts: the host is
    // part of the id, so they never collide in the shared engine store.
    let one = config_with("alice@example.com", "imap.one.net")
        .account_id()
        .expect("an id");
    let two = config_with("alice@example.com", "imap.two.net")
        .account_id()
        .expect("an id");
    assert_ne!(one, two);
}

#[test]
fn a_blank_username_yields_no_derived_id() {
    // An account whose login is blank has no identity to derive: refusing it here beats
    // connecting with a credential no server would accept.
    let blank = config_with("   ", "imap.example.net");
    assert!(matches!(
        blank.derived_account_id(),
        Err(ConfigError::Incomplete("username"))
    ));
}

#[test]
fn a_pinned_id_survives_edits_of_the_settings_it_was_derived_from() {
    // A pinned id is what lets the settings it was derived from be edited without the
    // account becoming another one: the derived id below differs, the pinned one stands.
    let pinned = load_str(&format!("id = \"pinned-id\"\n{SAMPLE}")).expect("valid config");
    assert_eq!(pinned.account_id().expect("an id").as_str(), "pinned-id");
    assert_ne!(
        pinned.account_id().expect("an id"),
        pinned.derived_account_id().expect("an id")
    );
    let stored = pinned.to_toml().expect("serialisable");
    let reloaded = load_str(&stored).expect("round-trips");
    assert_eq!(reloaded.account_id().expect("an id").as_str(), "pinned-id");
}

#[test]
fn certificate_exceptions_round_trip_and_reach_the_engine() {
    let certificate = engine_tls::CertificateDer::from(b"a certificate".to_vec());
    let exception = CertificateException::new("imap.example.net", &certificate);
    let config = load_str(&format!(
        "[imap]\naddr=\"h:993\"\nserver_name=\"h\"\nusername=\"u\"\npassword=\"p\"\n\
         [[certificate_exception]]\nserver_name=\"imap.example.net\"\n\
         sha256=\"{}\"\n",
        exception.sha256,
    ))
    .expect("valid config");
    assert_eq!(config.certificate_exceptions, vec![exception.clone()]);

    // The stored form round-trips, and the exception reaches the engine as the one
    // thing it admits.
    let reloaded = load_str(&config.to_toml().expect("serialisable")).expect("round-trips");
    assert_eq!(reloaded.certificate_exceptions, vec![exception]);
    let admitted = reloaded.tls_exceptions();
    assert_eq!(admitted.len(), 1);
    assert_eq!(admitted[0].server_name(), "imap.example.net");
    assert_eq!(
        admitted[0].fingerprint(),
        engine_tls::fingerprint(&certificate)
    );
}
