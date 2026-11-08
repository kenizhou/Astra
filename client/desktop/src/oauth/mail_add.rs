// SPDX-License-Identifier: GPL-3.0-only
//! The mail sign-in's account half: what a completed exchange's document
//! becomes once it answered, the IMAP/SMTP parallel of
//! [`super::jmap_add`] over the same recorded ordering.
//!
//! 1. **The exchange** validates the redirect against the state the
//!    begin minted and the `iss` the server advertised, refuses a token
//!    set with no refresh token (the door rule: an account that breaks
//!    an hour after setup is worse to debug than a refused sign-in),
//!    and assembles the `[imap]` document the grant belongs to, the
//!    accepted certificate riding and no password anywhere.
//! 2. **The re-consent check** reads the dial table by the derived id:
//!    an entry that stands means the grant is being renewed, and the
//!    stored shape keeps what it stored beside the old grant.
//! 3. **The entry registers BEFORE the dial**: the rotation sink reads
//!    the dial table's entry, so it must stand before the dial or a
//!    rotation during the dial has nowhere to land.
//! 4. **The persist reads the dial table, not the parsed copy**: the
//!    dial may already have rotated the refresh token through the sink,
//!    and writing the copy parsed a moment ago over that would restore
//!    the superseded token. A new id persists through the
//!    transaction's `add`; a re-consent goes through `rotate`, never
//!    `add`, whose rollback would delete a standing account's
//!    credential.
//! 5. **The account joins the app** without syncing, and the first sync
//!    runs in the background: the same assembly the boot performs.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use engine_api::{AccountId, EmailAddress};

use astra_account::{
    AccountConfig, AccountError, CertificateException, ConnectionSecurity, GraphTokenSource,
    ImapConnections,
};
use astra_oauth::{AuthStyle, OAuthClient, OAuthProviderConfig};

use crate::app_state::{AppPort, HostAccount, HostProvider};
use crate::dial_table::AccountDials;
use crate::runtime::AppRuntime;

use super::commands::RefusalKind;
use super::grant_refusals::indeterminate;
pub(crate) use super::grant_refusals::GraphAddFailure;
use super::mail_commands::MailSignInOutcome;
use super::transaction::CredentialTransaction;

/// The transient state carried between the begin and the completion,
/// round-tripped through the webview as the opaque pending handle. Never
/// persisted; it holds the PKCE verifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PendingMailLogin {
    /// The address the grant belongs to, the account's login name.
    pub(crate) email: String,
    /// The IMAP dial address (`host:port`) the account will dial.
    pub(crate) imap_addr: String,
    /// The TLS server name of the IMAP host.
    pub(crate) server_name: String,
    /// How the IMAP connection is secured.
    pub(crate) security: ConnectionSecurity,
    /// The certificate the person accepted for this server, when one was.
    pub(crate) accepted: Option<CertificateException>,
    /// The registered client id, static or dynamically minted.
    pub(crate) client_id: String,
    /// A client secret, on the rare server that issued one.
    pub(crate) client_secret: Option<String>,
    /// The discovered authorization endpoint.
    pub(crate) authorize_endpoint: String,
    /// The discovered token endpoint.
    pub(crate) token_endpoint: String,
    /// The redirect URI this sign-in was registered with.
    pub(crate) redirect_uri: String,
    /// The scopes requested.
    pub(crate) scopes: Vec<String>,
    /// The issuer the redirect's `iss` must name, when the server
    /// advertised that it sends one.
    pub(crate) issuer: Option<String>,
    /// The CSRF state the authorization request carried.
    pub(crate) state: String,
    /// The PKCE verifier the exchange sends.
    pub(crate) verifier: String,
}

/// The exchange half of a completed sign-in: validates the redirect,
/// exchanges the code, refuses a token set with no refresh token (the
/// door rule), and assembles the `[imap]` document the grant belongs
/// to.
///
/// # Errors
///
/// Answers [`GraphAddFailure`] for a malformed pending handle, a
/// refused exchange, or a server that issued no refresh token.
pub(crate) async fn complete_mail_login(
    pending: &str,
    callback_url: &str,
    now: OffsetDateTime,
) -> Result<String, GraphAddFailure> {
    let pending: PendingMailLogin = serde_json::from_str(pending).map_err(|err| {
        indeterminate(format!(
            "the pending sign-in handle could not be read: {err}"
        ))
    })?;
    let provider = OAuthProviderConfig {
        authorize_endpoint: pending.authorize_endpoint.clone(),
        token_endpoint: pending.token_endpoint.clone(),
        client_id: pending.client_id.clone(),
        client_secret: pending.client_secret.clone(),
        redirect_uri: pending.redirect_uri.clone(),
        scopes: pending.scopes.clone(),
        resource: None,
        expected_issuer: pending.issuer.clone(),
        style: AuthStyle::Discovered,
    };
    let client = OAuthClient::new(provider).map_err(|err| indeterminate(err.to_string()))?;
    log::info!(
        "mail oauth: redirect received; exchanging the code at {} (issuer check: {})",
        pending.token_endpoint,
        if pending.issuer.is_some() {
            "on"
        } else {
            "off"
        },
    );
    let tokens = client
        .complete(callback_url, &pending.state, &pending.verifier, now)
        .await
        .map_err(|err| GraphAddFailure {
            refusal: RefusalKind::from(err.refusal()),
            detail: err.to_string(),
        })?;
    // The door rule: `offline_access` is always requested, so a refresh
    // token is mandatory; without one the account would stop syncing
    // about an hour after setup, which is far worse to debug than
    // failing here.
    let refresh = tokens.refresh_token.ok_or_else(|| {
        log::warn!(
            "mail oauth: exchange succeeded but the server issued NO refresh token; granted \
             scope(s): [{}]",
            tokens.scope,
        );
        indeterminate(
            "the server issued no refresh token (offline_access was requested)".to_owned(),
        )
    })?;
    grant_document(&pending, refresh.expose())
}

/// The document a completed sign-in leaves: the `[imap]` section with
/// its grant and no password, the accepted certificate riding, the
/// STARTTLS choice written only where it was chosen.
///
/// # Errors
///
/// Answers [`GraphAddFailure`] when the document cannot be encoded.
fn grant_document(
    pending: &PendingMailLogin,
    refresh_token: &str,
) -> Result<String, GraphAddFailure> {
    let mut root = toml::Table::new();
    let mut imap = toml::Table::new();
    imap.insert("addr".into(), pending.imap_addr.clone().into());
    imap.insert("server_name".into(), pending.server_name.clone().into());
    imap.insert("username".into(), pending.email.clone().into());
    if pending.security == ConnectionSecurity::StartTls {
        imap.insert("security".into(), "start_tls".into());
    }
    let mut oauth = toml::Table::new();
    oauth.insert("client_id".into(), pending.client_id.clone().into());
    if let Some(secret) = &pending.client_secret {
        oauth.insert("client_secret".into(), secret.clone().into());
    }
    oauth.insert("refresh_token".into(), refresh_token.to_owned().into());
    oauth.insert(
        "authorize_endpoint".into(),
        pending.authorize_endpoint.clone().into(),
    );
    oauth.insert(
        "token_endpoint".into(),
        pending.token_endpoint.clone().into(),
    );
    oauth.insert("redirect_uri".into(), pending.redirect_uri.clone().into());
    oauth.insert(
        "scopes".into(),
        toml::Value::Array(pending.scopes.iter().cloned().map(Into::into).collect()),
    );
    if let Some(issuer) = &pending.issuer {
        oauth.insert("issuer".into(), issuer.clone().into());
    }
    imap.insert("oauth".into(), oauth.into());
    root.insert("imap".into(), imap.into());
    if let Some(accepted) = &pending.accepted {
        let mut exception = toml::Table::new();
        exception.insert("server_name".into(), accepted.server_name.clone().into());
        exception.insert("sha256".into(), accepted.sha256.clone().into());
        root.insert(
            "certificate_exception".into(),
            vec![toml::Value::Table(exception)].into(),
        );
    }
    toml::to_string(&root).map_err(|err| indeterminate(err.to_string()))
}

/// The host halves the completion drives.
pub(crate) struct MailAccountWiring {
    /// The app core: the account joins it without syncing.
    pub(crate) app: Arc<dyn AppPort>,
    /// The registered accounts: the pre-register's landing and the sink's
    /// read.
    pub(crate) dials: Arc<AccountDials>,
    /// The OS secure store the grant persists through, as a transaction
    /// per write.
    pub(crate) credentials: Arc<dyn crate::credential_store::AccountCredentialStore>,
    /// The runtime handle the first sync spawns on.
    pub(crate) spawner: tokio::runtime::Handle,
    /// The standing grant refusals the surfaces read.
    pub(crate) refusals: Arc<super::grant_refusals::GrantRefusals>,
}

impl MailAccountWiring {
    /// The wiring over one assembled state.
    #[must_use]
    pub(crate) fn from_state(state: &crate::AppState, runtime: &AppRuntime) -> Self {
        Self {
            app: Arc::clone(&state.app),
            dials: Arc::clone(&state.connections),
            credentials: Arc::clone(&state.credential_store),
            spawner: runtime.handle(),
            refusals: Arc::clone(&state.refusals),
        }
    }
}

/// The dial seam: the account's id, its shared token source, its stored
/// config, and the pool the dial connects; the bound providers out. The
/// one function every path that brings a grant-carrying mail account up
/// calls, so the completion and the boot cannot disagree about what such
/// an account opens.
pub(crate) type MailGrantDial = Arc<
    dyn Fn(
            &AccountId,
            Arc<GraphTokenSource>,
            &AccountConfig,
            &Arc<ImapConnections>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<HostProvider>, AccountError>> + Send>>
        + Send
        + Sync,
>;

/// The dial through the account layer: the credential source over the
/// sink-backed token source, installed into the pool so every later
/// connection of the account presents it, then the same folder-binding
/// connect every mail dial runs.
#[must_use]
pub(crate) fn production_mail_dial(crypto: Arc<astra_crypto_store::CryptoStore>) -> MailGrantDial {
    Arc::new(move |account, tokens, config, pool| {
        let wrapper = crate::crypto_wrapper::mime_wrapper_for_config(&crypto, config, account);
        let account = account.clone();
        let config = config.clone();
        let pool = Arc::clone(pool);
        Box::pin(async move {
            let section = config.imap.as_ref().ok_or(AccountError::NoImap)?;
            let source = astra_account::imap_oauth_credential_source(section, tokens)?;
            pool.install_credentials(source);
            astra_account::connect_mail_providers(&pool, &config, &account, wrapper).await
        })
    })
}

/// The completion a spawned mail sign-in half runs: the pending handle
/// and the raw redirect in, the outcome out. The production completion
/// spends them on the exchange and the account half beside it.
pub(crate) type MailCompletion = Box<
    dyn FnOnce(String, String) -> Pin<Box<dyn Future<Output = MailSignInOutcome> + Send>> + Send,
>;

/// The completion over the real exchange and the account half: the
/// document the exchange answers is registered, dialled, and persisted
/// exactly as the wizard's password leg adds an account.
pub(crate) fn production_mail_completion(
    state: &crate::AppState,
    runtime: &AppRuntime,
) -> MailCompletion {
    let wiring = MailAccountWiring::from_state(state, runtime);
    let dial = production_mail_dial(Arc::clone(&state.crypto_store));
    Box::new(move |pending, callback| {
        Box::pin(async move {
            let document =
                match complete_mail_login(&pending, &callback, OffsetDateTime::now_utc()).await {
                    Ok(document) => document,
                    Err(failure) => {
                        return MailSignInOutcome::Refused {
                            refusal: failure.refusal,
                            detail: failure.detail,
                        };
                    }
                };
            match complete_mail_signin(wiring, document, dial).await {
                Ok(address) => MailSignInOutcome::Added { address },
                Err(failure) => MailSignInOutcome::Refused {
                    refusal: failure.refusal,
                    detail: failure.detail,
                },
            }
        })
    })
}

/// Completes a mail sign-in into an account: the document parses, the
/// dial table entry registers **before** the dial (the rotation sink
/// reads it), the dial proves the grant works, the grant persists
/// through the transaction (an `add` for a new id, a `rotate` for a
/// re-consent), and the account joins the app with its first sync in
/// the background.
///
/// # Errors
///
/// Answers [`GraphAddFailure`] for every refusal along that path; the
/// rollback leaves nothing registered for a new id and the standing
/// entry exactly as it was for a re-consent. On success the answer is
/// the address the flow began with.
pub(crate) async fn complete_mail_signin(
    wiring: MailAccountWiring,
    document: String,
    dial: MailGrantDial,
) -> Result<String, GraphAddFailure> {
    let MailAccountWiring {
        app,
        dials,
        credentials,
        spawner,
        refusals,
    } = wiring;

    let mut config =
        astra_account::load_str(&document).map_err(|err| indeterminate(err.to_string()))?;
    let id = config
        .account_id()
        .map_err(|err| indeterminate(err.to_string()))?;
    let email = config
        .imap
        .as_ref()
        .map(|imap| imap.username.clone())
        .ok_or_else(|| indeterminate("the document stores no [imap] section".to_owned()))?;

    // Signing an existing account in again keeps what it stored beside
    // its grant, and its renewal persists through the rotation path: the
    // add path's rollback would delete a standing account's credential.
    let (stored, stored_pool) = match dials.of(id.as_str()) {
        Some((stored, pool)) => (Some(stored), Some(pool)),
        None => (None, None),
    };
    let reconsent = stored.is_some();
    keep_stored_shape(&mut config, stored.clone());

    // The sink reads the dial table's entry, so the entry must stand
    // BEFORE the dial: a rotation during the dial lands on the fresh
    // grant rather than nowhere. The same source the dial reads is the
    // one the sink advances, so there is one refresher of this
    // credential and not two.
    let sink = super::graph_sink::graph_token_sink(Arc::clone(&dials), Arc::clone(&credentials));
    let section = config
        .imap
        .as_ref()
        .ok_or_else(|| indeterminate("the document stores no [imap] section".to_owned()))?;
    let grant = section
        .oauth
        .as_ref()
        .ok_or_else(|| indeterminate("the document stores no [imap.oauth] grant".to_owned()))?;
    let tokens = astra_account::imap_oauth_token_source(
        grant,
        id.clone(),
        Some(sink),
        astra_account::CredentialOrigin::FreshSignIn,
    )
    .map_err(|err| indeterminate(err.to_string()))?;
    let pool = ImapConnections::new();
    if let Err(err) = dials.register(&id, config.clone(), Arc::clone(&pool)) {
        return Err(indeterminate(err.to_string()));
    }

    let providers = match dial(&id, tokens, &config, &pool).await {
        Ok(providers) => providers,
        // A dead grant during the dial is the dead word, never an outage.
        Err(err @ AccountError::SigninRejected(_)) => {
            roll_back(&dials, &id, stored.clone(), stored_pool.clone());
            if !reconsent {
                // A first add whose dial failed may have rotated through
                // the sink: the credential belongs to no account, so it
                // goes with the registration.
                let _ = CredentialTransaction::new(Arc::clone(&credentials)).remove(&id);
            }
            return Err(GraphAddFailure {
                refusal: RefusalKind::Dead,
                detail: err.to_string(),
            });
        }
        Err(err) => {
            roll_back(&dials, &id, stored.clone(), stored_pool.clone());
            if !reconsent {
                let _ = CredentialTransaction::new(Arc::clone(&credentials)).remove(&id);
            }
            return Err(indeterminate(err.to_string()));
        }
    };

    // The grant works, so store it; read back from the dial table, not
    // the copy parsed a moment ago, because the dial may already have
    // rotated the refresh token through the sink and writing the earlier
    // copy over that would restore the superseded token.
    let transaction = CredentialTransaction::new(Arc::clone(&credentials));
    let document = match dials.of(id.as_str()) {
        Some((persisted, _)) => persisted.to_toml().map_err(|err| {
            roll_back(&dials, &id, stored.clone(), stored_pool.clone());
            indeterminate(err.to_string())
        }),
        None => Err(indeterminate(
            "the account left the dial table while its grant was being stored".to_owned(),
        )),
    }?;
    let persist = if reconsent {
        transaction.rotate(&id, &document)
    } else {
        transaction.add(&id, &document)
    };
    if let Err(err) = persist {
        // The add path's own rollback already deleted best-effort; a
        // refused rotation leaves the previous document stored, which is
        // still what the next launch loads.
        roll_back(&dials, &id, stored, stored_pool);
        return Err(indeterminate(err.to_string()));
    }

    // The account joins the app without syncing, and the first sync runs
    // in the background: the same assembly the boot performs. A dial
    // that answered clears whatever refusal stood.
    refusals.record_dialled(id.as_str(), false);
    let account = HostAccount {
        id: id.clone(),
        providers,
        identity: EmailAddress::new(email.clone()),
    };
    let join_app = Arc::clone(&app);
    join_app.add_new_account_deferred(account).await;
    let sync_app = Arc::clone(&app);
    let sync_id = id.clone();
    spawner.spawn(async move { sync_app.sync_added_account(&sync_id).await });
    log::info!(
        "oauth: the mail sign-in completed into an account; the first sync runs in the background"
    );
    Ok(email)
}

mod shape;
use shape::{keep_stored_shape, roll_back};

#[cfg(test)]
#[path = "mail_add_tests.rs"]
mod tests;
