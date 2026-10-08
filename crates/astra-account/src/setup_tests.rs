// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the account-setup builder: the standard ports a bare host gains, the
//! scheme a bare CalDAV host gains, and the credentials that serialise into the TOML a
//! host stores.

use super::*;
use crate::load_str;

fn full_setup() -> AccountSetup {
    AccountSetup {
        // Bare hosts, no ports: the builder fills in the standard secure ports.
        imap_host: "imap.example.net".to_owned(),
        username: "me@example.net".to_owned(),
        // A password with TOML-special characters exercises the serializer's escaping.
        credential: SetupCredential::Password("p@ss\"with'quotes\\and=signs".to_owned()),
        smtp_host: Some("smtp.example.net".to_owned()),
        caldav_base_url: None,
        imap_security: ConnectionSecurity::ImplicitTls,
        smtp_security: ConnectionSecurity::ImplicitTls,
        accepted_certificate: None,
    }
}

#[test]
fn a_bare_host_gains_the_standard_ports_and_its_own_server_name() {
    let config = load_str(&build_config_toml(&full_setup()).unwrap()).unwrap();
    // A bare host gets the standard secure port; the TLS server name is the host.
    assert_eq!(config.imap.as_ref().unwrap().addr, "imap.example.net:993");
    assert_eq!(
        config.imap.as_ref().unwrap().server_name,
        "imap.example.net"
    );
    let smtp = config.smtp.as_ref().unwrap();
    assert_eq!(smtp.addr, "smtp.example.net:465");
    assert_eq!(smtp.server_name, "smtp.example.net");
}

#[test]
fn a_starttls_setup_gains_the_starttls_ports_and_records_the_security() {
    // A STARTTLS account with bare hosts gets the STARTTLS standard ports (143/587), and
    // the security round-trips through load_str so imap_config picks the STARTTLS
    // engine builders.
    let mut setup = full_setup();
    setup.imap_security = ConnectionSecurity::StartTls;
    setup.smtp_security = ConnectionSecurity::StartTls;
    let config = load_str(&build_config_toml(&setup).unwrap()).unwrap();
    assert_eq!(config.imap.as_ref().unwrap().addr, "imap.example.net:143");
    assert_eq!(
        config.imap.as_ref().unwrap().security,
        ConnectionSecurity::StartTls
    );
    let smtp = config.smtp.as_ref().unwrap();
    assert_eq!(smtp.addr, "smtp.example.net:587");
    assert_eq!(smtp.security, ConnectionSecurity::StartTls);
}

#[test]
fn an_implicit_tls_setup_writes_no_security_key() {
    // The default (implicit TLS) writes no `security` key, so a standard account's TOML
    // carries only what was chosen for it.
    let toml = build_config_toml(&full_setup()).unwrap();
    assert!(!toml.contains("security"));
}

#[test]
fn an_explicit_port_is_preserved() {
    let mut setup = full_setup();
    setup.imap_host = "imap.example.net:1993".to_owned();
    let config = load_str(&build_config_toml(&setup).unwrap()).unwrap();
    assert_eq!(config.imap.as_ref().unwrap().addr, "imap.example.net:1993");
    assert_eq!(
        config.imap.as_ref().unwrap().server_name,
        "imap.example.net"
    );
}

#[test]
fn a_bare_caldav_host_gains_the_https_scheme() {
    // A bare CalDAV host (no scheme), as a user types it, gets https:// prepended; the
    // calendar counterpart of the default-port leniency for IMAP/SMTP. The section is
    // written for the accounts slice that connects calendars.
    let mut bare = full_setup();
    bare.caldav_base_url = Some("caldav.example.net".to_owned());
    let toml = build_config_toml(&bare).unwrap();
    assert!(toml.contains("https://caldav.example.net"), "{toml}");

    // An explicit scheme (and path) is preserved.
    let mut explicit = full_setup();
    explicit.caldav_base_url = Some("http://dav.example.net/cal".to_owned());
    let toml = build_config_toml(&explicit).unwrap();
    assert!(toml.contains("http://dav.example.net/cal"), "{toml}");
}

#[test]
fn the_password_serialises_into_the_stored_toml() {
    let config = load_str(&build_config_toml(&full_setup()).unwrap()).unwrap();
    assert_eq!(config.imap.as_ref().unwrap().username, "me@example.net");
    // The password survives TOML escaping exactly (no plaintext seed file needed).
    assert_eq!(
        config
            .imap
            .as_ref()
            .unwrap()
            .password
            .as_ref()
            .unwrap()
            .expose(),
        "p@ss\"with'quotes\\and=signs"
    );
}

#[test]
fn unconfigured_optional_sections_are_omitted() {
    let mut setup = full_setup();
    setup.smtp_host = None;
    setup.caldav_base_url = None;
    let config = load_str(&build_config_toml(&setup).unwrap()).unwrap();
    assert!(config.smtp.is_none());
    let toml = build_config_toml(&setup).unwrap();
    assert!(!toml.contains("caldav"));
}

#[test]
fn a_missing_required_field_is_refused() {
    let mut setup = full_setup();
    setup.username = "  ".to_owned();
    assert!(matches!(
        build_config_toml(&setup),
        Err(ConfigError::Incomplete("username"))
    ));

    let mut setup = full_setup();
    setup.credential = SetupCredential::Password(String::new());
    assert!(matches!(
        build_config_toml(&setup),
        Err(ConfigError::Incomplete("password"))
    ));

    let mut setup = full_setup();
    setup.imap_host = String::new();
    assert!(matches!(
        build_config_toml(&setup),
        Err(ConfigError::Incomplete("mail server"))
    ));
}

/// An accepted certificate rides into the stored config, so every later connect of this
/// account carries it and nobody is asked twice.
#[test]
fn an_accepted_certificate_is_stored_and_reaches_the_engine() {
    let mut setup = full_setup();
    let certificate = engine_tls::CertificateDer::from(b"a certificate".to_vec());
    setup.accepted_certificate = Some(CertificateException::new("imap.example.net", &certificate));

    let config = load_str(&build_config_toml(&setup).unwrap()).unwrap();
    assert_eq!(
        config.certificate_exceptions,
        vec![CertificateException::new("imap.example.net", &certificate)]
    );
    let exceptions = config.tls_exceptions();
    assert_eq!(exceptions.len(), 1);
    assert_eq!(exceptions[0].server_name(), "imap.example.net");
    assert_eq!(
        exceptions[0].fingerprint(),
        engine_tls::fingerprint(&certificate)
    );
}

/// A setup that met no certificate problem writes no exception at all, so an ordinary
/// account's stored config carries nothing it did not choose.
#[test]
fn no_exception_is_written_when_none_was_accepted() {
    let toml = build_config_toml(&full_setup()).unwrap();
    assert!(!toml.contains("certificate_exception"));
    assert!(load_str(&toml).unwrap().certificate_exceptions.is_empty());
}

/// A sign-in grant builds an account that carries no stored password; the credential it
/// presents is refused at connect until the grant half is wired.
#[test]
fn a_grant_credential_builds_an_account_without_a_password() {
    let mut setup = full_setup();
    setup.credential = SetupCredential::Grant;
    let toml = build_config_toml(&setup).unwrap();
    assert!(!toml.contains("password"), "{toml}");
    let config = load_str(&toml).unwrap();
    assert!(config.imap.as_ref().unwrap().password.is_none());
    assert!(config.imap_password_credentials().is_none());
}

/// The setup types cross the host boundary as JSON, so their wire shape is pinned:
/// externally tagged credentials, snake_case, and optional fields omitted when empty.
#[test]
fn setup_types_cross_the_boundary_with_the_pinned_wire_shape() {
    let setup = full_setup();
    let wire = serde_json::to_value(&setup).unwrap();
    assert_eq!(
        wire["credential"]["password"],
        "p@ss\"with'quotes\\and=signs"
    );
    assert_eq!(wire["imap_security"], "implicit_tls");
    assert_eq!(wire["smtp_host"], "smtp.example.net");
    assert!(wire.get("caldav_base_url").is_none());
    assert!(wire.get("accepted_certificate").is_none());
    let parsed: AccountSetup = serde_json::from_value(wire).unwrap();
    assert_eq!(parsed.username, setup.username);
    assert_eq!(parsed.smtp_host, setup.smtp_host);

    // A credential with nothing set may also arrive: the tag alone names the kind.
    let grant = serde_json::to_value(SetupCredential::Grant).unwrap();
    assert_eq!(grant, serde_json::json!("grant"));
    let back: SetupCredential = serde_json::from_value(grant).unwrap();
    assert!(matches!(back, SetupCredential::Grant));
    let password: SetupCredential =
        serde_json::from_value(serde_json::json!({"password": "x"})).unwrap();
    assert!(matches!(password, SetupCredential::Password(p) if p == "x"));
}
