// SPDX-License-Identifier: GPL-3.0-only
//! Gated live test against the local Stalwart harness (docker/harness): the product's
//! IMAP connect path, over real TLS, against a server whose certificate nobody trusts.
//!
//! The harness serves implicit TLS on `127.0.0.1:2143` with a self-signed certificate,
//! so the first dial is refused as a certificate rejection and the test pins the
//! accepted certificate exactly as the product's owner would: a
//! [`CertificateException`] recorded for that one server. That is the recorded TLS
//! route for this harness (the decision sits in `docker/harness/compose.yaml` and
//! `scripts/dev/harness.sh`): the account layer dials TLS only, so a no-TLS port is
//! unreachable through the connect path under test.
//!
//! The test skips unless `ASTRA_LIVE_HARNESS=1`, so the offline `cargo test` stays
//! green. Against a running harness:
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! cargo test -p astra-app tests_live_harness -- --nocapture
//! ```

use engine_api::{AccountId, Provider, SyncScope};

use astra_account::{
    connect_mail_providers, AccountConfig, AccountError, AccountShape, CertificateException,
    ConnectionSecurity, ImapAccount, ImapConnections, Secret,
};

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
