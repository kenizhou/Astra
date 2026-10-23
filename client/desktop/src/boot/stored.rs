// SPDX-License-Identifier: GPL-3.0-only
//! The stored-account restore's dial half: the machinery that turns a
//! registered account document into the host-connected account the app
//! core boots over, whatever family its grant belongs to (an imap
//! password, a `[graph]` section, a `[google]` section), and the
//! on-demand folder binder the app calls back into for a late bind.
//!
//! Split from the boot sequence itself for the file cap: the ordering
//! the cached-first-paint contract rests on stays in
//! [`super::assemble_state`], and this module owns everything that
//! dials.

use std::future::Future;
use std::sync::Arc;

use engine_api::{AccountId, EmailAddress};

use crate::{dial_table::AccountDials, HostAccount, HostProvider};

// The on-demand folder binder (the app's MailboxConnector seam) lives in a
// sibling file, the same split the file cap forced here.
mod connector;

pub(crate) use connector::DialConnector;

/// The crypto store as every dial half holds it: the wrapper each IMAP
/// connect installs reads the one the boot opened.
pub(super) type SharedCryptoStore = Arc<astra_crypto_store::CryptoStore>;


/// Dials every registered account, mail half first and dav legs beside
/// it. A mail dial that fails skips the account with a warning: one stale
/// password must not block the others, and the app opens with whatever
/// connected. A dav dial that fails keeps the account standing on its mail
/// half alone, inside [`dial_dav_with`]. A Graph account dials through
/// `graph_dial` and a Google one through `google_dial`, the same free
/// functions the sign-ins' completions use, each over a sink-backed token
/// source built from its stored grant; what its dial answered lands in
/// `refusals`, so a dead grant leaves the expired sign-in standing for
/// the row the boot's skip would otherwise orphan.
pub(crate) async fn dial_stored(
    dials: &Arc<AccountDials>,
    credentials: &Arc<dyn crate::credential_store::AccountCredentialStore>,
    crypto_store: &SharedCryptoStore,
    graph_dial: crate::oauth::graph_add::GraphDial,
    google_dial: crate::oauth::google_add::GoogleDial,
    refusals: &Arc<crate::oauth::grant_refusals::GrantRefusals>,
) -> Vec<Dialled> {
    let mut accounts = Vec::new();
    for id in dials.ids() {
        let Some((config, connections)) = dials.of(&id) else {
            continue;
        };
        let account_id = match config.account_id() {
            Ok(account_id) => account_id,
            Err(err) => {
                log::warn!("boot: an account could not derive its id and was skipped: {err}");
                continue;
            }
        };
        // A legacy config the fail-closed settings write now refuses: warn,
        // never fail the boot.
        if config.crypto.is_some() && (config.imap.is_none() || config.smtp.is_none()) {
            log::warn!(
                "boot: an account carries a crypto method but has no IMAP/SMTP submission settings; its sends go out plainly until the method is cleared"
            );
        }
        if config.imap.is_none() && config.graph.is_some() {
            match dial_graph_stored(
                dials,
                credentials,
                &graph_dial,
                &account_id,
                &config,
                refusals,
            )
            .await
            {
                Ok(account) => accounts.push(Dialled {
                    account,
                    contacts: None,
                }),
                // The grant is dead: the account skips, and the refusal
                // state it leaves behind is the only account of why.
                Err(astra_account::AccountError::SigninRejected(detail)) => {
                    refusals.record_dead(account_id.as_str());
                    log::warn!(
                        "boot: an account's sign-in expired or was withdrawn; it waits to be \
                         signed in again: {detail}"
                    );
                }
                Err(err) => {
                    log::warn!("boot: an account could not be connected and was skipped: {err}");
                }
            }
            continue;
        }
        if config.imap.is_none() && config.google.is_some() {
            match dial_google_stored(
                dials,
                credentials,
                &google_dial,
                &account_id,
                &config,
                refusals,
            )
            .await
            {
                Ok(account) => accounts.push(Dialled {
                    account,
                    contacts: None,
                }),
                // The grant is dead: the account skips, and the refusal
                // state it leaves behind is the only account of why.
                Err(astra_account::AccountError::SigninRejected(detail)) => {
                    refusals.record_dead(account_id.as_str());
                    log::warn!(
                        "boot: an account's sign-in expired or was withdrawn; it waits to be \
                         signed in again: {detail}"
                    );
                }
                Err(err) => {
                    log::warn!("boot: an account could not be connected and was skipped: {err}");
                }
            }
            continue;
        }
        if config.imap.is_none() && config.jmap.is_some() {
            match dial_jmap_stored(dials, credentials, &account_id, &config).await {
                Ok(account) => {
                    let (account, contacts) = dial_dav(&config, account).await;
                    accounts.push(Dialled { account, contacts });
                }
                // The credential was refused outright (a dead grant's
                // token mint or a password the session's own 401
                // rejects): the account skips, and the refusal state it
                // leaves behind is the row the surfaces render.
                Err(astra_account::AccountError::SigninRejected(detail)) => {
                    refusals.record_dead(account_id.as_str());
                    log::warn!(
                        "boot: an account's sign-in expired or was withdrawn; it waits to be \
                         signed in again: {detail}"
                    );
                }
                Err(err) => {
                    log::warn!("boot: an account could not be connected and was skipped: {err}");
                }
            }
            continue;
        }
        let email = config
            .imap
            .as_ref()
            .map(|imap| imap.username.clone())
            .unwrap_or_default();
        let wrapper =
            crate::crypto_wrapper::mime_wrapper_for_config(crypto_store, &config, &account_id);
        match astra_account::connect_mail_providers(&connections, &config, &account_id, wrapper)
            .await
        {
            Ok(providers) => {
                let account = HostAccount {
                    id: account_id,
                    providers,
                    identity: EmailAddress::new(email),
                };
                let (account, contacts) = dial_dav(&config, account).await;
                accounts.push(Dialled { account, contacts });
            }
            Err(err) => {
                log::warn!("boot: an account could not be connected and was skipped: {err}");
            }
        }
    }
    accounts
}

/// Dials one stored Graph account: the sink-backed token source over the
/// stored refresh token, then the shared dial. The account joins with its
/// calendar appended after its mail providers, the shape the
/// capability-scoped calendar selection expects; a calendar that failed
/// costs the boot nothing, its error carried in the dial's outcome for
/// the log, and what the dial answered about the grant lands in
/// `refusals` for the surfaces: a withheld calendar records the
/// reconnect state, a healthy dial records none.
async fn dial_graph_stored(
    dials: &Arc<AccountDials>,
    credentials: &Arc<dyn crate::credential_store::AccountCredentialStore>,
    graph_dial: &crate::oauth::graph_add::GraphDial,
    account_id: &AccountId,
    config: &astra_account::AccountConfig,
    refusals: &Arc<crate::oauth::grant_refusals::GrantRefusals>,
) -> Result<HostAccount, astra_account::AccountError> {
    let graph = config
        .graph
        .as_ref()
        .ok_or(astra_account::AccountError::NoImap)?;
    // The stored grant dials through the same sink the sign-in's
    // completion registers: a rotation at boot lands in the entry and the
    // store, and a stored source adopts whatever this process already
    // holds for the account.
    let sink =
        crate::oauth::graph_sink::graph_token_sink(Arc::clone(dials), Arc::clone(credentials));
    let tokens = astra_account::GraphTokenSource::new(
        &graph.provider_config(),
        account_id.clone(),
        graph.refresh_token.expose(),
        Some(sink),
        astra_account::CredentialOrigin::Stored,
    )?;
    let dialled = graph_dial(account_id, tokens, config).await?;
    refusals.record_dialled(account_id.as_str(), dialled.calendar_reauth_required);
    if let Some(err) = &dialled.calendar_error {
        if dialled.calendar_reauth_required {
            log::warn!("boot: a graph calendar needs a fresh sign-in; the agenda waits: {err}");
        } else {
            log::warn!("boot: a graph calendar failed to dial; the mail stands: {err}");
        }
    }
    let mut providers = dialled.mail;
    if let Some(calendar) = dialled.calendar {
        providers.push(calendar);
    }
    Ok(HostAccount {
        id: account_id.clone(),
        providers,
        identity: EmailAddress::new(graph.email.clone()),
    })
}

/// Dials one stored Google account: the sink-backed token source over
/// the stored refresh token, then the shared dial. The account joins
/// with its calendar appended after its account-global mail provider,
/// the shape the capability-scoped calendar selection expects; a
/// calendar that failed costs the boot nothing, its error carried in the
/// dial's outcome for the log. There is no re-auth classification to
/// make on the way out: Google asks for the calendar scope at sign-in,
/// so a dial that answered clears whatever refusal stood and a refused
/// calendar is an error, never a banner.
async fn dial_google_stored(
    dials: &Arc<AccountDials>,
    credentials: &Arc<dyn crate::credential_store::AccountCredentialStore>,
    google_dial: &crate::oauth::google_add::GoogleDial,
    account_id: &AccountId,
    config: &astra_account::AccountConfig,
    refusals: &Arc<crate::oauth::grant_refusals::GrantRefusals>,
) -> Result<HostAccount, astra_account::AccountError> {
    let google = config
        .google
        .as_ref()
        .ok_or(astra_account::AccountError::NoImap)?;
    // The stored grant dials through the same sink the sign-in's
    // completion registers: a rotation at boot lands in the entry and the
    // store, and a stored source adopts whatever this process already
    // holds for the account.
    let sink =
        crate::oauth::graph_sink::graph_token_sink(Arc::clone(dials), Arc::clone(credentials));
    let tokens = astra_account::google_token_source(
        google,
        account_id.clone(),
        Some(sink),
        astra_account::CredentialOrigin::Stored,
    )?;
    let dialled = google_dial(account_id, tokens, config).await?;
    refusals.record_dialled(account_id.as_str(), false);
    if let Some(err) = &dialled.calendar_error {
        log::warn!("boot: a google calendar failed to dial; the mail stands: {err}");
    }
    let mut providers = dialled.mail;
    if let Some(calendar) = dialled.calendar {
        providers.push(calendar);
    }
    Ok(HostAccount {
        id: account_id.clone(),
        providers,
        identity: EmailAddress::new(google.email.clone()),
    })
}

/// Dials one stored JMAP account: the one account-wide provider, over
/// the stored password or over the grant's sink-backed token source
/// (a rotation a ratcheting server issues at this dial lands in the
/// entry and the store, the defence without which the account dies at
/// the next launch). The dial is the account layer's own free function
/// both shapes walk, so the boot, the folder binder, and the sign-in's
/// completion cannot disagree about what a JMAP account opens.
async fn dial_jmap_stored(
    dials: &Arc<AccountDials>,
    credentials: &Arc<dyn crate::credential_store::AccountCredentialStore>,
    account_id: &AccountId,
    config: &astra_account::AccountConfig,
) -> Result<HostAccount, astra_account::AccountError> {
    let jmap = config
        .jmap
        .as_ref()
        .ok_or(astra_account::AccountError::NoImap)?;
    let tls = astra_account::account_tls(config)?;
    let provider: HostProvider = match jmap.oauth.as_ref() {
        Some(_) => {
            // The stored grant dials through the same sink the sign-in's
            // completion registers: a rotation at boot lands in the
            // entry and the store, and a stored source adopts whatever
            // this process already holds for the account.
            let sink = crate::oauth::graph_sink::graph_token_sink(
                Arc::clone(dials),
                Arc::clone(credentials),
            );
            let source = astra_account::jmap_token_source(
                jmap,
                account_id.clone(),
                Some(sink),
                astra_account::CredentialOrigin::Stored,
            )?;
            Box::new(astra_account::dial_jmap_oauth_account(config, &tls, &source).await?)
        }
        None => Box::new(astra_account::dial_jmap_account(config, &tls).await?),
    };
    Ok(HostAccount {
        id: account_id.clone(),
        providers: vec![provider],
        identity: EmailAddress::new(jmap.email.clone()),
    })
}

/// One dialled account as the boot holds it: the account with every dialled
/// provider installed, and the contacts leg to install beside it once the
/// app core exists.
pub(crate) struct Dialled {
    /// The account as the app installs it: the mail providers first, the
    /// appended calendar provider after.
    pub(crate) account: HostAccount,
    /// The dialled contacts leg, when the account has one.
    pub(crate) contacts: Option<HostContactsLeg>,
}

/// The contacts leg type the boot dials and installs: a CardDAV provider
/// boxed behind the engine's contacts domain, the one shape the mail-generic
/// app core can drive.
pub(crate) type HostContactsLeg = Box<dyn engine_api::ContactsProvider>;

/// Splits the dialled accounts into the list the app core boots over and the
/// contacts legs to install after it: the legs cannot ride the accounts
/// (their providers list is the mail-and-calendar surface), and they cannot
/// be installed before the core exists.
pub(crate) fn split_dialled(
    dialled: Vec<Dialled>,
) -> (Vec<HostAccount>, Vec<(AccountId, HostContactsLeg)>) {
    let mut accounts = Vec::new();
    let mut legs = Vec::new();
    for dialled in dialled {
        if let Some(leg) = dialled.contacts {
            legs.push((dialled.account.id.clone(), leg));
        }
        accounts.push(dialled.account);
    }
    (accounts, legs)
}

/// Dials the account's dav legs through the real connect and folds them
/// into `account`.
async fn dial_dav(
    config: &astra_account::AccountConfig,
    account: HostAccount,
) -> (HostAccount, Option<HostContactsLeg>) {
    dial_dav_with(config, account, || {
        astra_account::connect_dav_providers(config)
    })
    .await
}

/// Folds a dialled dav pair into `account`: the CalDAV provider appended
/// after the mail providers (the calendar paths pick it by capability,
/// wherever in the list it stands), the CardDAV provider handed back as the
/// contacts leg to install. A failed dial leaves the account standing on
/// its mail half alone, logged: a server that splits them or a section that
/// names an unreachable origin costs the account neither its mail nor the
/// boot. An account with no dav section dials nothing at all.
pub(super) async fn dial_dav_with<F, Fut>(
    config: &astra_account::AccountConfig,
    mut account: HostAccount,
    dial: F,
) -> (HostAccount, Option<HostContactsLeg>)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<astra_account::DavProviders, astra_account::AccountError>>,
{
    if config.dav.is_none() {
        return (account, None);
    }
    match dial().await {
        Ok(dav) => {
            if let Some(caldav) = dav.caldav {
                account.providers.push(caldav);
            }
            (account, dav.carddav)
        }
        Err(err) => {
            log::warn!(
                "boot: an account's dav legs could not be connected; the mail half stands alone: {err}"
            );
            (account, None)
        }
    }
}
