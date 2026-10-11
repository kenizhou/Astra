// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the dav half of a stored account: the optional section that
//! names the calendar and contacts origins, the rule that decides which legs
//! a section yields, and the secrets that never reach a log line.
//!
//! The connect path itself dials a real server (the engine's adapters take
//! no offline seam at their public surface), so these tests pin the
//! selection the dial walks and leave the live proof to the gated harness
//! suite.

use crate::{load_str, ConfigError};

/// A minimal mail-only account, the way every stored account stood before
/// the dav section existed.
fn config_with(username: &str, server_name: &str) -> crate::AccountConfig {
    load_str(&format!(
        "[imap]\naddr=\"{server_name}:993\"\nserver_name=\"{server_name}\"\n\
         username=\"{username}\"\npassword=\"p\"\n",
    ))
    .expect("valid config")
}

/// A dav section naming both origins, the way a split server's does.
const SPLIT: &str = r#"
[imap]
addr = "imap.example.net:993"
server_name = "imap.example.net"
username = "you@example.net"
password = "hunter2"

[dav]
caldav_url = "https://cal.example.net"
carddav_url = "https://card.example.net"
username = "you@example.net"
password = "hunter2"
"#;

/// A dav section naming only the calendar origin, the way nearly every
/// server's does: one origin that speaks both dialects.
const SHARED_ORIGIN: &str = r#"
[imap]
addr = "imap.example.net:993"
server_name = "imap.example.net"
username = "you@example.net"
password = "hunter2"

[dav]
caldav_url = "https://dav.example.net"
username = "you@example.net"
password = "hunter2"
"#;

#[test]
fn a_dav_section_loads_beside_the_mail_half() {
    let config = load_str(SPLIT).expect("valid config");
    let dav = config.dav.as_ref().expect("the dav section loads");
    assert_eq!(
        dav.caldav_url.as_deref(),
        Some("https://cal.example.net"),
        "the calendar origin loads as stored"
    );
    assert_eq!(
        dav.carddav_url.as_deref(),
        Some("https://card.example.net"),
        "the contacts origin loads as stored"
    );
    assert_eq!(dav.username, "you@example.net");
    assert_eq!(dav.password.as_ref().expect("password").expose(), "hunter2");
    // The mail half stands untouched beside it.
    assert!(config.imap.is_some() && config.smtp.is_none());
}

#[test]
fn an_account_without_a_dav_section_loads_as_it_always_did() {
    let config = config_with("you@example.net", "imap.example.net");
    assert!(config.dav.is_none(), "no section, no legs");
}

#[test]
fn a_dav_section_without_any_origin_is_refused() {
    let neither = r#"
[imap]
addr = "imap.example.net:993"
server_name = "imap.example.net"
username = "you@example.net"

[dav]
username = "you@example.net"
"#;
    assert!(
        matches!(load_str(neither), Err(ConfigError::NoDavOrigin)),
        "a section that names no server has nothing to dial"
    );
    // A blank origin is no origin: whitespace names no server either.
    let blank = r#"
[imap]
addr = "imap.example.net:993"
server_name = "imap.example.net"
username = "you@example.net"

[dav]
caldav_url = "  "
username = "you@example.net"
"#;
    assert!(matches!(load_str(blank), Err(ConfigError::NoDavOrigin)));
}

#[test]
fn the_carddav_leg_falls_back_to_the_caldav_origin() {
    let shared = load_str(SHARED_ORIGIN).expect("valid config");
    let dav = shared.dav.as_ref().expect("the dav section");
    assert_eq!(dav.caldav_origin(), Some("https://dav.example.net"));
    assert_eq!(
        dav.carddav_origin(),
        Some("https://dav.example.net"),
        "no origin of its own dials the calendar's"
    );

    let split = load_str(SPLIT).expect("valid config");
    let dav = split.dav.as_ref().expect("the dav section");
    assert_eq!(
        dav.carddav_origin(),
        Some("https://card.example.net"),
        "an origin of its own wins over the fallback"
    );

    // A contacts-only section dials no calendar leg at all.
    let contacts_only = load_str(
        "[imap]\naddr=\"imap.example.net:993\"\nserver_name=\"imap.example.net\"\n\
         username=\"you@example.net\"\n\n[dav]\ncarddav_url=\"https://card.example.net\"\n\
         username=\"you@example.net\"\n",
    )
    .expect("valid config");
    let dav = contacts_only.dav.as_ref().expect("the dav section");
    assert_eq!(dav.caldav_origin(), None, "no calendar origin stands");
    assert_eq!(dav.carddav_origin(), Some("https://card.example.net"));
}

#[test]
fn a_dav_section_round_trips_through_the_stored_form() {
    let config = load_str(SPLIT).expect("valid config");
    let stored = config.to_toml().expect("serialisable config");
    let parsed = load_str(&stored).expect("round-trips");
    let dav = parsed.dav.as_ref().expect("the dav section survives");
    assert_eq!(dav.caldav_url.as_deref(), Some("https://cal.example.net"));
    assert_eq!(dav.carddav_url.as_deref(), Some("https://card.example.net"));
    assert_eq!(dav.password.as_ref().expect("password").expose(), "hunter2");

    // A contacts-only section round-trips without inventing a calendar leg.
    let contacts_only = load_str(
        "[imap]\naddr=\"imap.example.net:993\"\nserver_name=\"imap.example.net\"\n\
         username=\"you@example.net\"\n\n[dav]\ncarddav_url=\"https://card.example.net\"\n\
         username=\"you@example.net\"\npassword=\"p\"\n",
    )
    .expect("valid config");
    let stored = contacts_only.to_toml().expect("serialisable config");
    assert!(
        !stored.contains("caldav_url"),
        "no leg crosses unasked: {stored}"
    );
    let parsed = load_str(&stored).expect("round-trips");
    let dav = parsed.dav.as_ref().expect("the dav section survives");
    assert!(dav.caldav_url.is_none());
    assert_eq!(dav.carddav_url.as_deref(), Some("https://card.example.net"));

    // A section with no password round-trips without inventing the key: a
    // stored document without one loads as far as the dial, which is where
    // the missing credential is answered.
    let no_password = load_str(
        "[imap]\naddr=\"imap.example.net:993\"\nserver_name=\"imap.example.net\"\n\
         username=\"you@example.net\"\n\n[dav]\ncaldav_url=\"https://cal.example.net\"\n\
         username=\"you@example.net\"\n",
    )
    .expect("valid config");
    let stored = no_password.to_toml().expect("serialisable config");
    assert!(
        !stored.contains("password"),
        "the absent key crosses unasked: {stored}"
    );
    let parsed = load_str(&stored).expect("round-trips");
    let dav = parsed.dav.as_ref().expect("the dav section survives");
    assert!(dav.password.is_none());
}

#[test]
fn the_dav_secret_stays_out_of_the_debug_line() {
    let config = load_str(SPLIT).expect("valid config");
    let rendered = format!("{config:?}");
    assert!(rendered.contains("Secret(<redacted>)"));
    assert!(!rendered.contains("hunter2"), "a log line never carries it");
}

#[test]
fn a_dav_section_without_a_password_names_no_credential() {
    let no_password = load_str(
        "[imap]\naddr=\"imap.example.net:993\"\nserver_name=\"imap.example.net\"\n\
         username=\"you@example.net\"\n\n[dav]\ncaldav_url=\"https://dav.example.net\"\n\
         username=\"you@example.net\"\n",
    )
    .expect("valid config");
    let dav = no_password.dav.as_ref().expect("the dav section");
    assert!(
        dav.credentials().is_none(),
        "a section storing no password has nothing to present"
    );

    let with = load_str(SHARED_ORIGIN).expect("valid config");
    let dav = with.dav.as_ref().expect("the dav section");
    let credentials = dav.credentials().expect("the stored password stands");
    match credentials {
        provider_caldav::Credentials::Basic { username, password } => {
            assert_eq!(username, "you@example.net");
            assert_eq!(
                password, "hunter2",
                "the dav legs present basic credentials"
            );
        }
        other => panic!("the dav legs present basic credentials, not {other:?}"),
    }
}
