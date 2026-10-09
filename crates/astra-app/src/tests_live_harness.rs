// SPDX-License-Identifier: GPL-3.0-only
//! Gated live tests against the local Stalwart harness (docker/harness): the
//! product's IMAP connect path, over real TLS, against a server whose certificate
//! nobody trusts; and the product's detection path, over the real HTTP fetcher,
//! against a server whose domain resolves nowhere.
//!
//! The harness serves implicit TLS on `127.0.0.1:2143` with a self-signed certificate,
//! so the first dial is refused as a certificate rejection and the test pins the
//! accepted certificate exactly as the product's owner would: a
//! [`CertificateException`] recorded for that one server. That is the recorded TLS
//! route for this harness (the decision sits in `docker/harness/compose.yaml` and
//! `scripts/dev/harness.sh`): the account layer dials TLS only, so a no-TLS port is
//! unreachable through the connect path under test.
//!
//! The JMAP-detection test cannot dial `https://example.test/.well-known/jmap` (the
//! harness domain resolves nowhere), so it rebases the probe onto the harness's plain
//! HTTP JMAP origin (`127.0.0.1:2080`) through the detection crate's dev-harness
//! `well_known_base_override`, the seam whose waiver that crate's unit tests already
//! pin. This mirrors mail-calendar's live JMAP tests, which publish the same
//! plaintext listener; no host trust store is touched.
//!
//! The tests skip unless `ASTRA_LIVE_HARNESS=1`, so the offline `cargo test` stays
//! green. Against a running harness:
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! cargo test -p astra-app tests_live_harness -- --nocapture
//! ```

use std::time::Duration;

use engine_api::{AccountId, Provider, SyncScope};

use astra_account::{
    connect_mail_providers, AccountConfig, AccountError, AccountShape, CertificateException,
    ConnectionSecurity, ImapAccount, ImapConnections, Secret,
};
use astra_autodetect::{detect, DetectConfig, Detected, SourceKind};

/// The harness connection details from the environment, or `None` to skip.
fn harness() -> Option<(String, String, String, String)> {
    if std::env::var("ASTRA_LIVE_HARNESS").ok().as_deref() != Some("1") {
        return None;
    }
    let host = std::env::var("ASTRA_HARNESS_IMAP_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let port = std::env::var("ASTRA_HARNESS_IMAP_PORT").unwrap_or_else(|_| "2143".to_owned());
    let user = std::env::var("ASTRA_HARNESS_IMAP_USER")
        .unwrap_or_else(|_| "astra1@example.test".to_owned());
    let password =
        std::env::var("ASTRA_HARNESS_IMAP_PASSWORD").unwrap_or_else(|_| "astra-harness".to_owned());
    Some((host, port, user, password))
}

/// The harness's plain-HTTP JMAP origin from the environment, or `None` to skip.
fn jmap_base() -> Option<String> {
    if std::env::var("ASTRA_LIVE_HARNESS").ok().as_deref() != Some("1") {
        return None;
    }
    Some(
        std::env::var("ASTRA_HARNESS_JMAP_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:2080".to_owned()),
    )
}

/// The harness account, with the certificate exceptions its owner has accepted (none
/// for the first dial).
fn account(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
    exceptions: Vec<CertificateException>,
) -> AccountConfig {
    AccountConfig {
        imap: Some(ImapAccount {
            addr: format!("{host}:{port}"),
            server_name: host.to_owned(),
            username: user.to_owned(),
            password: Some(Secret::new(password.to_owned())),
            security: ConnectionSecurity::ImplicitTls,
        }),
        smtp: None,
        certificate_exceptions: exceptions,
        shape: AccountShape::default(),
    }
}

/// The mailbox each provider is bound to, in the order the dial returned them.
fn bound_mailboxes(providers: &[Box<dyn Provider>], account_id: &AccountId) -> Vec<String> {
    providers
        .iter()
        .map(|provider| match provider.email_scope(account_id) {
            SyncScope::ImapMailbox { mailbox, .. } => mailbox.as_str().to_owned(),
            other => panic!("an IMAP folder provider bound to {other:?}"),
        })
        .collect()
}

/// The dial against the harness walks the certificate acceptance path a person does:
/// refused first, the rejection recorded, the one certificate accepted for the one
/// server, and then the dial binds the INBOX and the custom folder the seed created.
#[tokio::test]
async fn a_dial_pins_the_rejected_certificate_and_binds_the_inbox_and_the_custom_folder() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let config = account(&host, &port, &user, &password, Vec::new());
    let account_id = config.account_id().expect("the account id");
    let connections = ImapConnections::new();

    let Err(error) = connect_mail_providers(&connections, &config, &account_id).await else {
        panic!("the harness certificate was trusted without an accepted exception");
    };
    let AccountError::CertificateRejected { rejected, .. } = error else {
        panic!("the self-signed certificate was not offered as a rejection: {error}");
    };

    let accepted = CertificateException::new(rejected.server_name(), rejected.certificate());
    let config = account(&host, &port, &user, &password, vec![accepted]);

    let providers = connect_mail_providers(&connections, &config, &account_id)
        .await
        .expect("the dial with the accepted certificate");

    let bound = bound_mailboxes(&providers, &account_id);
    assert_eq!(
        bound.first().map(String::as_str),
        Some("INBOX"),
        "the reserved INBOX is bound first; the bound list is {bound:?}",
    );
    assert!(
        bound.contains(&"Projects".to_owned()),
        "the custom folder astra1 seeds is bound; the bound list is {bound:?}",
    );
}

/// Detection for a harness user runs the real well-known probe over the real HTTP
/// fetcher: `/.well-known/jmap` redirects to the session, the session's anonymous
/// JSON advertises `capabilities`, and the account layer routes the find onto the
/// JMAP form the wizard prefills. The run is clamped to the host command's five
/// second bound, so it also proves the probe fits that budget.
#[tokio::test]
async fn detection_returns_the_jmap_recommendation_for_a_harness_user() {
    let Some(base) = jmap_base() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let config = DetectConfig {
        overall_deadline: Duration::from_secs(5),
        well_known_base_override: Some(url::Url::parse(&base).expect("the harness JMAP URL")),
        ..DetectConfig::default()
    };

    let email = "astra1@example.test";
    let detected = detect(email, None, &config)
        .await
        .expect("the detection run against the harness");

    let Detected::Jmap(jmap) = detected else {
        panic!("the harness domain speaks JMAP over its well-known, got {detected:?}");
    };
    assert_eq!(jmap.base_url, base, "the stored base is the harness origin");
    assert!(
        jmap.is_trusted,
        "the harness override is the sanctioned trusted route"
    );
    assert_eq!(
        jmap.source.kind,
        SourceKind::JmapWellKnown,
        "the apex well-known (rebased) is the winning source"
    );

    let recommendation = astra_account::recommend(
        email,
        Detected::Jmap(jmap),
        astra_account::OauthRoutes {
            google: false,
            microsoft: false,
        },
    );
    let astra_account::SetupRecommendation::Jmap { server_url, .. } = recommendation else {
        panic!("a JMAP find routes to the JMAP form, got {recommendation:?}");
    };
    assert_eq!(
        server_url, base,
        "the JMAP form is prefilled with the harness origin"
    );
}
