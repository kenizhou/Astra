// SPDX-License-Identifier: GPL-3.0-only
//! The installed credential source's tests: a grant-carrying account
//! dials over the source its pool holds, a reconnect after a dropped
//! socket presents the same source, and a grant account with no source
//! installed is refused rather than dialling with nothing.

use std::sync::Arc;

use super::*;

/// A credential source stand-in answering one fixed OAuth credential: the
/// installed-source arm's token source, with no network behind it. The
/// scripted server answers any authenticate line, so the credential's shape
/// is all the dial observes.
#[derive(Debug)]
struct FixedToken;

/// The capability line the installed-source tests' server advertises:
/// the OAuth SASL mechanism a grant-carrying account's dial negotiates.
const OAUTH_CAPABILITIES: &str = "IMAP4rev1 IDLE SPECIAL-USE AUTH=OAUTHBEARER";

#[async_trait::async_trait]
impl provider_imap::CredentialSource for FixedToken {
    async fn credentials(
        &self,
    ) -> Result<provider_imap::Credentials, engine_provider::ProviderError> {
        Ok(provider_imap::Credentials::oauth2(
            "someone@example.com",
            "AT-1",
        ))
    }

    async fn renew(
        &self,
        _refused: &provider_imap::Credentials,
    ) -> Result<Option<provider_imap::Credentials>, engine_provider::ProviderError> {
        Ok(None)
    }
}

/// A stored `[imap.oauth]` grant account pointed at the server, storing no
/// password: the shape a browser sign-in persists, whose every connection
/// presents the installed source's token.
fn grant_account(port: u16, served: &rustls::pki_types::CertificateDer<'static>) -> AccountConfig {
    let exception = CertificateException::new("127.0.0.1", served);
    load_str(&format!(
        "[imap]\naddr = \"127.0.0.1:{port}\"\nserver_name = \"127.0.0.1\"\n\
         username = \"someone@example.com\"\n\
         \n[imap.oauth]\nclient_id = \"client-abc\"\nrefresh_token = \"rt-1\"\n\
         authorize_endpoint = \"https://auth.example.test/authorize\"\n\
         token_endpoint = \"https://auth.example.test/token\"\n\
         redirect_uri = \"com.kenizhou.astra://oauth\"\nscopes = [\"offline_access\"]\n\
         \n[[certificate_exception]]\nserver_name = \"{}\"\nsha256 = \"{}\"\n",
        exception.server_name, exception.sha256,
    ))
    .expect("a valid grant account")
}

#[tokio::test]
async fn a_grant_account_dials_over_its_installed_credential_source() {
    // The pool is the one place an account's credential source can stand:
    // the dial installs it once, and the engine asks it on every connection
    // the account opens. Without it a grant account has no password to
    // derive, and the dial refuses rather than guessing.
    let (port, served, _accepted) = scripted_server_offering(OAUTH_CAPABILITIES).await;
    let config = grant_account(port, &served);
    let connections = ImapConnections::new();
    connections.install_credentials(Arc::new(FixedToken));

    let providers = connect_mail_providers(&connections, &config, &account_id(), None)
        .await
        .expect("the grant account dials over the installed source");

    assert_eq!(providers.len(), 7, "every listed folder binds");
}

#[tokio::test]
async fn a_grant_account_without_an_installed_source_is_refused() {
    // The password path is unchanged: a config storing no password and no
    // installed source has no credential to present, and the refusal names
    // it rather than dialling with nothing.
    let (port, served, _accepted) = scripted_server_offering(OAUTH_CAPABILITIES).await;
    let config = grant_account(port, &served);

    let error = first_login(&config).await;
    assert!(
        matches!(error, AccountError::MissingCredential(_)),
        "a grant account with no installed source dials with nothing: {error}"
    );
}

#[tokio::test]
async fn a_reconnect_after_invalidate_still_presents_the_installed_source() {
    // Sockets drop and the pool dials again; the installed source is the
    // pool's own, so the reconnect presents the same credential instead of
    // deriving one from a config that stores no password.
    let (port, served, accepted) = scripted_server_offering(OAUTH_CAPABILITIES).await;
    let config = grant_account(port, &served);
    let connections = ImapConnections::new();
    connections.install_credentials(Arc::new(FixedToken));
    let providers = connect_mail_providers(&connections, &config, &account_id(), None)
        .await
        .expect("the first dial");

    connections.invalidate();
    providers[0]
        .sync_mailboxes(&account_id(), None)
        .await
        .expect("the folder list, over a reconnected account");

    assert_eq!(
        accepted.load(Ordering::SeqCst),
        2,
        "the reconnect dialled again and presented the installed source"
    );
}
