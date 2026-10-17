// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the jmap account section: the one-secret fold (the legacy
//! `token` key reads into `password`), the URL derivation matrix, the
//! account id, and the `[jmap.oauth]` block a browser sign-in leaves. The
//! fold is pinned hardest: a stored document never carries two secret
//! fields, and a document that once did still loads.

use provider_jmap::Credentials;

use super::{base_host, build_jmap_config_toml, jmap_base_url, normalize_base_url, JmapSetup};
use crate::{load_str, AccountConfig, ConfigError};

/// A `[jmap]` document with a password, the form every setup writes.
fn password_doc() -> &'static str {
    "[jmap]\nemail = \"Alice@Example.com\"\nbase_url = \"https://mail.example.com\"\npassword = \"hunter2\"\n"
}

/// The loaded form of [`password_doc`].
fn password_account() -> AccountConfig {
    load_str(password_doc()).expect("a valid jmap document")
}

/// A document written before the one-secret collapse: the secret sits
/// under `token`, and it must keep working rather than silently lose its
/// secret.
fn legacy_token_doc() -> &'static str {
    "[jmap]\nemail = \"me@example.net\"\nbase_url = \"https://api.example.net\"\ntoken = \"legacy-tok\"\n"
}

/// A setup form for the derivation matrix.
fn setup(server_url: Option<&str>, password: &str) -> JmapSetup {
    JmapSetup {
        email: "me@example.net".to_owned(),
        server_url: server_url.map(str::to_owned),
        password: password.to_owned(),
    }
}

/// A document with a complete `[jmap.oauth]` block, the form a browser
/// sign-in writes.
fn oauth_doc() -> &'static str {
    "[jmap]\nemail = \"alice@example.com\"\nbase_url = \"https://api.example.com\"\n\n[jmap.oauth]\nclient_id = \"client-abc\"\nrefresh_token = \"rt-1\"\nauthorize_endpoint = \"https://api.example.com/oauth/authorize\"\ntoken_endpoint = \"https://api.example.com/oauth/refresh\"\nredirect_uri = \"com.kenizhou.astra://jmap-oauth\"\nscopes = [\"offline_access\"]\nresource = \"https://api.example.com/jmap/session\"\n"
}

#[test]
fn account_id_lowercases_and_tags_the_scheme_and_host() {
    assert_eq!(
        password_account()
            .account_id()
            .expect("a derivable id")
            .as_str(),
        "alice@example.com@jmap:mail.example.com",
    );
}

#[test]
fn account_id_is_distinct_from_imap_and_the_oauth_families_for_the_same_address() {
    let jmap = password_account().account_id().expect("a derivable id");
    let imap = engine_api::AccountId::try_from("alice@example.com@mail.example.com").unwrap();
    let graph = engine_api::AccountId::try_from("alice@example.com@graph").unwrap();
    let google = engine_api::AccountId::try_from("alice@example.com@google").unwrap();
    assert_ne!(jmap, imap);
    assert_ne!(jmap, graph);
    assert_ne!(jmap, google);
}

#[test]
fn account_id_differs_by_server_for_the_same_address() {
    let a = password_account().account_id().expect("a derivable id");
    let b = load_str(
        "[jmap]\nemail = \"Alice@Example.com\"\nbase_url = \"https://jmap.other.net\"\npassword = \"hunter2\"\n",
    )
    .expect("a valid jmap document");
    assert_ne!(a, b.account_id().expect("a derivable id"));
}

#[test]
fn base_host_strips_scheme_and_path_and_keeps_the_port() {
    assert_eq!(base_host("http://127.0.0.1:18080"), "127.0.0.1:18080");
    assert_eq!(base_host("https://mail.example.com/"), "mail.example.com");
    assert_eq!(
        base_host("https://Mail.Example.com/jmap"),
        "mail.example.com",
    );
    // A scheme-less host (tolerated) still yields the host.
    assert_eq!(base_host("mail.example.com"), "mail.example.com");
}

#[test]
fn to_toml_round_trips_a_password_account() {
    let toml = password_account().to_toml().expect("serialisable");
    let parsed = load_str(&toml).expect("a valid jmap document");
    let jmap = parsed.jmap.as_ref().expect("the jmap section");
    assert_eq!(jmap.email, "Alice@Example.com");
    assert_eq!(jmap.base_url, "https://mail.example.com");
    assert_eq!(
        jmap.password.as_ref().expect("the secret").expose(),
        "hunter2"
    );
    // And the document round-trips byte for byte: the section writes only
    // what it read.
    assert_eq!(toml, parsed.to_toml().expect("serialisable"));
}

#[test]
fn a_config_stored_before_the_collapse_still_loads_its_secret() {
    // Backward compatibility: an account set up under the old two-field
    // form persisted `token = ...`. It must keep working rather than
    // silently losing its secret: the fold changes what is written, never
    // what is read.
    let config = load_str(legacy_token_doc()).expect("a valid jmap document");
    let jmap = config.jmap.as_ref().expect("the jmap section");
    assert_eq!(
        jmap.password.as_ref().expect("the folded secret").expose(),
        "legacy-tok",
    );
}

#[test]
fn a_legacy_token_rewrites_as_the_one_secret_and_never_as_two() {
    // The stored form a loaded legacy account writes back: the secret
    // lands under `password`, and the `token` key never reappears.
    let config = load_str(legacy_token_doc()).expect("a valid jmap document");
    let toml = config.to_toml().expect("serialisable");
    assert!(toml.contains("password = \"legacy-tok\""), "{toml}");
    assert!(!toml.contains("token"), "{toml}");
    let reloaded = load_str(&toml).expect("a valid jmap document");
    assert_eq!(
        reloaded
            .jmap
            .as_ref()
            .expect("the jmap section")
            .password
            .as_ref()
            .expect("the secret")
            .expose(),
        "legacy-tok",
    );
}

#[test]
fn a_document_with_both_secret_keys_keeps_the_token_value() {
    // A document carrying both keys (no writer produces one) keeps the
    // `token` value: the secret its account was last connected with.
    let config = load_str(
        "[jmap]\nemail = \"me@example.net\"\nbase_url = \"https://api.example.net\"\npassword = \"old-pw\"\ntoken = \"live-tok\"\n",
    )
    .expect("a valid jmap document");
    let jmap = config.jmap.as_ref().expect("the jmap section");
    assert_eq!(
        jmap.password.as_ref().expect("the folded secret").expose(),
        "live-tok",
    );
}

#[test]
fn basic_credentials_use_the_email_as_username() {
    let account = password_account();
    let jmap = account.jmap.as_ref().expect("the jmap section");
    match jmap.credentials().expect("a stored secret") {
        Credentials::Basic { username, password } => {
            assert_eq!(username, "Alice@Example.com");
            assert_eq!(password, "hunter2");
        }
        Credentials::Bearer(_) => panic!("expected the username-bearing form"),
    }
}

#[test]
fn a_section_without_a_secret_presents_no_credentials() {
    let config =
        load_str("[jmap]\nemail = \"me@example.net\"\nbase_url = \"https://api.example.net\"\n")
            .expect("a valid jmap document");
    let jmap = config.jmap.as_ref().expect("the jmap section");
    assert!(jmap.password.is_none());
    assert!(jmap.credentials().is_none());
}

#[test]
fn an_api_token_is_stored_as_a_password_so_it_can_be_presented_under_either_scheme() {
    // The whole point of one field: whatever the user pastes, a login
    // password, an app password, or an API token, is stored in the
    // username-bearing form. A bare token could only ever go out as
    // Bearer, so a server that challenges Basic would reject it; this
    // form works against both.
    let toml =
        build_jmap_config_toml(&setup(Some("api.example.net"), "fmapi-token")).expect("built");
    // The key never appears in the stored form (the secret's own value may
    // spell the word; the key is what a document carries).
    assert!(!toml.contains("\ntoken ="), "{toml}");
    let config = load_str(&toml).expect("a valid jmap document");
    let jmap = config.jmap.as_ref().expect("the jmap section");
    assert_eq!(
        jmap.password.as_ref().expect("the secret").expose(),
        "fmapi-token",
    );
    match jmap.credentials().expect("a stored secret") {
        Credentials::Basic { username, password } => {
            assert_eq!(username, "me@example.net");
            assert_eq!(password, "fmapi-token");
        }
        Credentials::Bearer(_) => panic!("a setup secret is never stored bearer-only"),
    }
}

#[test]
fn debug_never_leaks_the_secret() {
    let config = load_str(legacy_token_doc()).expect("a valid jmap document");
    assert!(!format!("{config:?}").contains("legacy-tok"));
    let password = load_str(password_doc()).expect("a valid jmap document");
    assert!(!format!("{password:?}").contains("hunter2"));
}

#[test]
fn the_oauth_block_reads_none_for_every_account_stored_today() {
    // No writer emits the block yet, so every stored account reads `None`,
    // and a stored document stays free of the key.
    let config = load_str(password_doc()).expect("a valid jmap document");
    let jmap = config.jmap.as_ref().expect("the jmap section");
    assert!(jmap.oauth.is_none());
    let toml = config.to_toml().expect("serialisable");
    assert!(!toml.contains("oauth"), "{toml}");
}

#[test]
fn an_oauth_section_round_trips_through_the_stored_document() {
    // The block a browser sign-in leaves: the discovered endpoints, the
    // registered client id, and the refresh token, with no long-lived
    // password beside them. It survives a load+save field for field.
    let config = load_str(oauth_doc()).expect("a valid jmap document");
    let jmap = config.jmap.as_ref().expect("the jmap section");
    let grant = jmap.oauth.as_ref().expect("the grant");
    assert_eq!(grant.client_id, "client-abc");
    assert_eq!(grant.refresh_token.expose(), "rt-1");
    assert_eq!(
        grant.authorize_endpoint,
        "https://api.example.com/oauth/authorize"
    );
    assert_eq!(
        grant.token_endpoint,
        "https://api.example.com/oauth/refresh"
    );
    assert_eq!(grant.redirect_uri, "com.kenizhou.astra://jmap-oauth");
    assert_eq!(
        grant.resource.as_deref(),
        Some("https://api.example.com/jmap/session"),
    );
    assert_eq!(grant.scopes, vec!["offline_access".to_owned()]);
    let rewritten = config.to_toml().expect("serialisable");
    let reloaded = load_str(&rewritten).expect("a valid jmap document");
    assert_eq!(
        reloaded
            .jmap
            .as_ref()
            .expect("the jmap section")
            .oauth
            .as_ref()
            .expect("the round-tripped grant")
            .refresh_token
            .expose(),
        "rt-1",
    );
    // The byte-for-byte rule the password account keeps: what was read is
    // what is written back.
    assert_eq!(rewritten, reloaded.to_toml().expect("serialisable"));
}

#[test]
fn an_oauth_block_from_a_newer_build_still_loads() {
    // What a newer build owes this one is that the load never fails over
    // keys it does not read: an unknown key inside the block is tolerated,
    // and the optional keys (the secret, the resource, the issuer) stay
    // optional.
    let config = load_str(
        "[jmap]\nemail = \"me@example.net\"\nbase_url = \"https://api.example.net\"\n\n[jmap.oauth]\nclient_id = \"client-abc\"\nrefresh_token = \"rt-1\"\nauthorize_endpoint = \"https://api.example.com/oauth/authorize\"\ntoken_endpoint = \"https://api.example.com/oauth/refresh\"\nredirect_uri = \"com.kenizhou.astra://jmap-oauth\"\nscopes = [\"offline_access\"]\na_newer_key = \"whatever\"\n",
    )
    .expect("a valid jmap document");
    let jmap = config.jmap.as_ref().expect("the jmap section");
    let grant = jmap.oauth.as_ref().expect("the grant");
    assert!(grant.client_secret.is_none());
    assert!(grant.resource.is_none());
    assert!(grant.issuer.is_none());
    assert!(!format!("{config:?}").contains("rt-1"));
}

#[test]
fn a_partial_oauth_block_is_refused_rather_than_halved() {
    // A block missing a required field (here the client id) is a broken
    // document, not a grant worth half its fields: an OAuth account whose
    // client id was dropped could never refresh again.
    let err = load_str(
        "[jmap]\nemail = \"me@example.net\"\nbase_url = \"https://api.example.net\"\n\n[jmap.oauth]\nrefresh_token = \"rt-1\"\nauthorize_endpoint = \"https://api.example.com/oauth/authorize\"\ntoken_endpoint = \"https://api.example.com/oauth/refresh\"\nredirect_uri = \"com.kenizhou.astra://jmap-oauth\"\nscopes = [\"offline_access\"]\n",
    );
    assert!(err.is_err(), "a grant without a client id loaded");
}

// The URL derivation matrix: what the setup form collected against the
// base URL the account dials.

#[test]
fn build_toml_with_an_explicit_server_and_password() {
    let config =
        load_str(&build_jmap_config_toml(&setup(Some("mail.example.net"), "pw")).expect("built"))
            .expect("a valid jmap document");
    // A bare host gets an https:// scheme.
    let jmap = config.jmap.as_ref().expect("the jmap section");
    assert_eq!(jmap.base_url, "https://mail.example.net");
    assert_eq!(jmap.email, "me@example.net");
    assert_eq!(jmap.password.as_ref().expect("the secret").expose(), "pw");
}

#[test]
fn build_toml_preserves_an_explicit_http_scheme_for_a_local_server() {
    let config = load_str(
        &build_jmap_config_toml(&setup(Some("http://127.0.0.1:18080"), "pw")).expect("built"),
    )
    .expect("a valid jmap document");
    let jmap = config.jmap.as_ref().expect("the jmap section");
    assert_eq!(jmap.base_url, "http://127.0.0.1:18080");
}

#[test]
fn an_explicit_http_scheme_on_a_public_host_is_upgraded_to_https() {
    // Real accounts are always TLS: plaintext survives only for the
    // loopback fixture, so a public host typed with `http://` is upgraded
    // rather than dialled in the clear.
    for server in ["http://mail.example.net", "HTTP://Mail.Example.net:443"] {
        let url = jmap_base_url("me@example.net", Some(server)).expect("derived");
        assert!(url.starts_with("https://"), "{server} -> {url}");
    }
    // The loopback hosts keep their plaintext form, harness and all.
    for server in [
        "http://127.0.0.1:2080",
        "http://localhost:2080",
        "http://[::1]:2080",
    ] {
        let url = jmap_base_url("me@example.net", Some(server)).expect("derived");
        assert!(url.starts_with("http://"), "{server} -> {url}");
    }
}

#[test]
fn build_toml_derives_the_server_from_the_email_domain_when_blank() {
    let config =
        load_str(&build_jmap_config_toml(&setup(None, "pw")).expect("built")).expect("built");
    // Empty server URL derives https://<domain> for /.well-known/jmap
    // discovery.
    let jmap = config.jmap.as_ref().expect("the jmap section");
    assert_eq!(jmap.base_url, "https://example.net");
}

#[test]
fn a_blank_server_with_no_domain_in_the_email_cannot_derive_a_url() {
    assert!(matches!(
        jmap_base_url("no-at-sign", None),
        Err(ConfigError::Incomplete("server URL"))
    ));
    assert!(matches!(
        jmap_base_url("me@", None),
        Err(ConfigError::Incomplete("server URL"))
    ));
    // A whitespace-only server URL counts as blank.
    assert_eq!(
        jmap_base_url("me@example.net", Some("   ")).expect("derived"),
        "https://example.net",
    );
}

#[test]
fn build_toml_requires_a_secret() {
    assert!(matches!(
        build_jmap_config_toml(&setup(Some("mail.example.net"), "")),
        Err(ConfigError::Incomplete("password"))
    ));
    // A whitespace-only secret counts as absent.
    assert!(matches!(
        build_jmap_config_toml(&setup(Some("mail.example.net"), "  ")),
        Err(ConfigError::Incomplete("password"))
    ));
}

#[test]
fn build_toml_requires_an_email() {
    let mut empty = setup(Some("mail.example.net"), "pw");
    empty.email = "  ".to_owned();
    assert!(matches!(
        build_jmap_config_toml(&empty),
        Err(ConfigError::Incomplete("email"))
    ));
}

#[test]
fn an_explicit_https_base_passes_through_unchanged() {
    // The scheme the owner typed is theirs to keep: only http is subject
    // to the loopback rule, and nothing rewrites a path or port away.
    let url = normalize_base_url("https://mail.example.test:443/jmap");
    assert_eq!(url, "https://mail.example.test:443/jmap");
}
