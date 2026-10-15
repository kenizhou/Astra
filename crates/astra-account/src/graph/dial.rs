// SPDX-License-Identifier: GPL-3.0-only
//! The Microsoft account dial: the one function every path that brings a
//! Graph account up calls (the boot, the sign-in's completion, a
//! reconnect), assembling the account's providers over the shared token
//! source.
//!
//! Mail binds one refreshing provider per folder the account lists that
//! the account pass syncs (the engine's Graph adapter is folder-bound,
//! exactly as its IMAP one is), and the calendar binds one refreshing
//! provider to the account's default calendar, appended after the mail
//! providers the way a dav account's calendar leg is: the host account's
//! capability-scoped selection picks it from the list. Both legs spend
//! the same token, so one refresh serves the whole dial.

use std::sync::Arc;

use engine_api::{AccountId, MailboxId, Provider, TimeZoneId};
use engine_core::sync::SyncUpdate;
use engine_tls::TlsClientConfig;
use provider_graph::{GraphClient, GraphProvider, MailboxPrincipal};
use time::Date;

use super::calendar::connect_graph_calendar_providers;
use super::provider::RefreshingGraphProvider;
use super::GraphTokenSource;
use crate::{account_log_handle, account_tls, pass_syncs, AccountConfig, AccountError};

/// The providers a Microsoft account dials, and the two things about its
/// calendar leg a caller may need to act on.
pub struct MicrosoftProviders {
    /// The account's mail providers: one per listed folder the account
    /// pass syncs, each folder-bound the way the engine's Graph adapter
    /// requires. An account pass syncs what is bound, so a folder left
    /// out would be in no pass.
    pub mail: Vec<Box<dyn Provider>>,
    /// The default calendar's provider, when the account's calendar
    /// connected; appended after the mail providers, the shape the
    /// capability-scoped calendar selection expects.
    pub calendar: Option<Box<dyn Provider>>,
    /// The account-labelled error from a calendar connect that failed;
    /// the account's mail stands and the agenda is empty. Carried rather
    /// than logged away so a caller can surface it (a boot diagnostic, a
    /// settings row); an empty calendar is otherwise indistinguishable
    /// from a server that has none.
    pub calendar_error: Option<String>,
    /// Whether the calendar was withheld by a scope-denied `403`, i.e.
    /// the account's grant predates the calendar scope and the user must
    /// re-consent. `false` for every other calendar outcome and every
    /// mail outcome.
    pub calendar_reauth_required: bool,
}

impl core::fmt::Debug for MicrosoftProviders {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The providers hold live connections; the dialled legs are the
        // whole safe description.
        f.debug_struct("MicrosoftProviders")
            .field("mail", &self.mail.len())
            .field("calendar", &self.calendar.is_some())
            .field("calendar_error", &self.calendar_error)
            .field("calendar_reauth_required", &self.calendar_reauth_required)
            .finish()
    }
}

/// Dials a Microsoft account's providers: the mail providers for its
/// folders and the provider for its default calendar, over the one
/// shared token source. The one function every path that brings a Graph
/// account up calls, so boot, sign-in, and reconnect cannot disagree
/// about what a Microsoft account opens.
///
/// The mail leg is the account's: its failure fails the dial. The
/// calendar leg is not: a calendar that fails (a scope-denied `403`, an
/// unreachable probe) leaves the account up with an empty agenda, and
/// the failure is carried in the outcome for the caller to surface.
///
/// `account` is the account's stored document: both legs run under its
/// shared TLS policy, so the certificates its owner accepted pin into
/// the Graph connections exactly as they do the mail half's. The
/// credential half of the document arrived earlier, as `tokens`.
///
/// # Errors
///
/// Returns [`AccountError`] when the mail leg fails: the initial token
/// refresh (a dead grant arrives as [`AccountError::SigninRejected`]) or
/// the folder-list sync.
pub async fn dial_microsoft_account(
    account_id: &AccountId,
    tokens: Arc<GraphTokenSource>,
    account: &AccountConfig,
    display_zone: TimeZoneId,
) -> Result<MicrosoftProviders, AccountError> {
    let tls = account_tls(account)?;
    // The same Graph token serves both legs, concurrently: the probes
    // share the source's single refresh flight, so a dial mints at most
    // one access token however many legs it opens.
    let (mail, calendar) = tokio::join!(
        connect_graph_mail_providers(account_id, Arc::clone(&tokens), None, &tls),
        connect_graph_calendar_providers(account_id, Arc::clone(&tokens), display_zone, &tls),
    );
    let mail = mail?;
    let (calendar, calendar_error, calendar_reauth_required) = match calendar {
        Ok(provider) => (Some(provider), None, false),
        Err(err @ AccountError::CalendarAccessDenied(_)) => {
            log::warn!("graph: calendar access denied; re-consent needed: {err}");
            (None, Some(err.to_string()), true)
        }
        Err(err) => {
            log::warn!("graph: the calendar leg failed to dial; mail stands: {err}");
            (None, Some(err.to_string()), false)
        }
    };
    Ok(MicrosoftProviders {
        mail,
        calendar,
        calendar_error,
        calendar_reauth_required,
    })
}

/// Builds a Graph provider bound to one folder of a Microsoft account:
/// one the account listed after it connected, or one opened before any
/// pass synced it, sharing the account's `tokens`. The token is minted
/// lazily on the first call.
///
/// # Errors
///
/// Returns [`AccountError::Mailbox`] if `mailbox_key` is not a valid
/// folder id.
pub fn connect_graph_folder(
    tokens: Arc<GraphTokenSource>,
    mailbox_key: &str,
    since: Option<Date>,
    tls: &TlsClientConfig,
) -> Result<Box<dyn Provider>, AccountError> {
    let folder =
        MailboxId::try_from(mailbox_key).map_err(|err| AccountError::Mailbox(err.to_string()))?;
    Ok(Box::new(RefreshingGraphProvider::new(
        folder,
        tokens,
        since,
        tls.clone(),
    )))
}

/// Connects the Graph mail providers a Microsoft account syncs: one per
/// folder the account lists that [`pass_syncs`], the Graph parallel of
/// [`connect_mail_providers`](crate::connect_mail_providers), and bound
/// in full for the same reason, since an account pass syncs what is
/// bound and nothing else.
///
/// # Errors
///
/// Returns [`AccountError`] if the initial token refresh or the
/// folder-list sync fails.
pub async fn connect_graph_mail_providers(
    account_id: &AccountId,
    tokens: Arc<GraphTokenSource>,
    since: Option<Date>,
    tls: &TlsClientConfig,
) -> Result<Vec<Box<dyn Provider>>, AccountError> {
    let folders = list_folders(&tokens, account_id, tls).await?;
    Ok(folders
        .into_iter()
        .filter(pass_syncs)
        .map(|mailbox| {
            Box::new(RefreshingGraphProvider::new(
                mailbox.id,
                Arc::clone(&tokens),
                since,
                tls.clone(),
            )) as Box<dyn Provider>
        })
        .collect())
}

/// Enumerates the account's mail folders (a full snapshot), using a
/// one-off client with a fresh access token. The bound folder is
/// irrelevant to the folder-list call, so any well-known alias serves to
/// construct the probe.
async fn list_folders(
    tokens: &Arc<GraphTokenSource>,
    account_id: &AccountId,
    tls: &TlsClientConfig,
) -> Result<Vec<engine_api::Mailbox>, AccountError> {
    let token = tokens.access_token().await?;
    let client = GraphClient::for_mailbox(token, MailboxPrincipal::Me, tls, &tokens.retry())
        .map_err(|err| AccountError::Graph(err.to_string()))?;
    let inbox =
        MailboxId::try_from("inbox").map_err(|err| AccountError::Mailbox(err.to_string()))?;
    // Both log lines carry the account's handle. One device holds several
    // Microsoft accounts, this runs once per account, and two counts
    // minutes apart read as one mailbox's list shrinking unless each says
    // whose it was. The handle, never the id: an account id is an
    // address and a host.
    let handle = account_log_handle(account_id.as_str());
    log::debug!("graph[{handle}]: fetching the mail folder list");
    let listing = GraphProvider::new(client, inbox)
        .sync_mailboxes(account_id, None)
        .await
        .map_err(|err| AccountError::MailboxList(err.to_string()))?;
    let folders = match listing.update {
        SyncUpdate::Snapshot { objects, .. } => objects,
        SyncUpdate::Delta { changed, .. } => changed,
    };
    log::debug!(
        "graph[{handle}]: the folder list returned {} folder(s)",
        folders.len(),
    );
    Ok(folders)
}

#[cfg(test)]
#[path = "dial_tests.rs"]
mod tests;
