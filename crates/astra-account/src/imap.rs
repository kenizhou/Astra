// SPDX-License-Identifier: GPL-3.0-only
//! An IMAP account's connections, and the providers the app binds to them.
//!
//! The engine bounds an account's sockets only when every folder is bound through **one**
//! [`provider_imap::ImapAccount`]: it holds the account's connection budget, and a second one for
//! the same account is a second budget. [`ImapConnections`] is where that one lives, one per
//! registered account, so the dial and every folder bound after it both draw from it.

use std::sync::{Arc, Mutex};

use engine_api::{AccountId, MailboxId, MailboxRole, MimeWrapper, Provider};
use engine_core::sync::SyncUpdate;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

use crate::{
    imap_credential_source, pass_syncs,
    reconnect::{ReconnectingImapProvider, Redial},
    tls::account_tls,
    AccountConfig, AccountError,
};

/// The engine's account type over the TLS stream every live connection uses.
type LiveImapAccount = provider_imap::ImapAccount<TlsStream<TcpStream>>;

/// One registered IMAP account's connections: the engine account every folder provider of it
/// is bound through, connected on first use.
///
/// Build one per registered account and keep it beside the account's config. A replaced config (a
/// new password) is a new registration and so a new `ImapConnections`, which is what keeps a
/// pool dialled with the old credential from serving the new one.
#[derive(Default)]
pub struct ImapConnections {
    /// The connected account, once there is one. A plain mutex: held only to read or swap the
    /// `Arc`, never across a connect.
    account: Mutex<Option<Arc<LiveImapAccount>>>,
    /// Held across a connect, so two callers finding no account open one between them.
    connecting: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for ImapConnections {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImapConnections")
            .field("connected", &self.current().is_some())
            .finish_non_exhaustive()
    }
}

impl ImapConnections {
    /// No account connected yet: the first dial connects it.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Drops every resting connection, so the next call dials fresh. For a host that saw the
    /// device come back online: the sockets from before look open and are not.
    pub fn invalidate(&self) {
        if let Some(account) = self.current() {
            account.invalidate();
        }
    }

    fn current(&self) -> Option<Arc<LiveImapAccount>> {
        self.account
            .lock()
            .expect("imap account mutex poisoned")
            .clone()
    }

    /// Connects the account afresh and makes it the one every later call is bound through.
    ///
    /// This login is the account's **first** in the dial, so a refusal here is the one that can
    /// mean the password is wrong ([`AccountError::SigninRejected`]). The account it
    /// replaces keeps serving whatever is still bound to it, minus its resting connections.
    async fn connect(
        &self,
        config: &AccountConfig,
        mime_wrapper: Option<Arc<dyn MimeWrapper>>,
    ) -> Result<Arc<LiveImapAccount>, AccountError> {
        let _connecting = self.connecting.lock().await;
        let account = Arc::new(connect_account(config, mime_wrapper).await?);
        let replaced = self
            .account
            .lock()
            .expect("imap account mutex poisoned")
            .replace(Arc::clone(&account));
        if let Some(replaced) = replaced {
            replaced.invalidate();
        }
        Ok(account)
    }

    /// The connected account, connecting it first when nothing has yet.
    async fn current_or_connect(
        &self,
        config: &AccountConfig,
        mime_wrapper: Option<Arc<dyn MimeWrapper>>,
    ) -> Result<Arc<LiveImapAccount>, AccountError> {
        if let Some(account) = self.current() {
            return Ok(account);
        }
        let _connecting = self.connecting.lock().await;
        // Another caller may have connected while this one waited for the lock.
        if let Some(account) = self.current() {
            return Ok(account);
        }
        let account = Arc::new(connect_account(config, mime_wrapper).await?);
        *self.account.lock().expect("imap account mutex poisoned") = Some(Arc::clone(&account));
        Ok(account)
    }

    /// Opens the account's standing INBOX watch, the delivery loop's
    /// latency channel, when the server offers `IDLE` and the pool has the
    /// headroom for a watch that leaves syncing a connection to sync with.
    /// Every other answer is [`WatchFace::Poll`]: a server without the
    /// capability and a pool at its watch ceiling are facts, not failures,
    /// and the account's cadence carries its delivery.
    ///
    /// The watch sends nothing, so no MIME wrapper is installed.
    ///
    /// # Errors
    ///
    /// Returns [`AccountError`] when the account's connection fails, or the
    /// engine's classified failure for a watch that refused outside the
    /// poll answers above.
    pub async fn watch_inbox(
        &self,
        config: &AccountConfig,
        keepalive: std::time::Duration,
    ) -> Result<crate::watch::WatchFace, AccountError> {
        let imap = self.current_or_connect(config, None).await?;
        let inbox = mailbox_id("INBOX")?;
        if !imap
            .provider(inbox.clone())
            .connection_info()
            .capabilities
            .idle()
        {
            return Ok(crate::watch::WatchFace::Poll);
        }
        if imap.watch_headroom() == 0 {
            return Ok(crate::watch::WatchFace::Poll);
        }
        match imap.watch(inbox, keepalive).await {
            Ok(watcher) => Ok(crate::watch::WatchFace::Watch(Box::new(watcher))),
            Err(err) if err.class() == engine_api::FailureClass::InvalidState => {
                Ok(crate::watch::WatchFace::Poll)
            }
            Err(err) => Err(AccountError::Watch(err)),
        }
    }
}

/// Logs in over the account's TLS policy, reading a refusal as the first login's.
///
/// Every connection the account dials later asks the same credential source, so the account's
/// hundredth connection presents the credential that is valid when it is dialled.
async fn connect_account(
    config: &AccountConfig,
    mime_wrapper: Option<Arc<dyn MimeWrapper>>,
) -> Result<LiveImapAccount, AccountError> {
    let tls = account_tls(config)?;
    let credentials = imap_credential_source(config)?;
    let mut imap = config.imap_config(credentials)?;
    if let Some(wrapper) = mime_wrapper {
        imap = imap.with_mime_wrapper(wrapper);
    }
    LiveImapAccount::connect(&imap, tls.connector())
        .await
        .map_err(|err| AccountError::from_first_imap_login(err).over_tls(&tls))
}

/// Parses a stored mailbox key.
fn mailbox_id(mailbox: &str) -> Result<MailboxId, AccountError> {
    MailboxId::try_from(mailbox).map_err(|err| AccountError::Mailbox(err.to_string()))
}

/// Binds `mailbox` of an account through its `imap` connections, wrapped so a call that fails on
/// a dead socket is retried once.
fn bind(imap: &Arc<LiveImapAccount>, mailbox: MailboxId) -> Box<dyn Provider> {
    let provider: Arc<dyn Provider> = Arc::new(imap.provider(mailbox.clone()));
    let redial = make_imap_redial(Arc::clone(imap), mailbox.clone());
    Box::new(ReconnectingImapProvider::adopt(provider, mailbox, redial))
}

/// The [`Redial`] a [`ReconnectingImapProvider`] runs after a call failed on a dead socket.
///
/// Binding a folder dials nothing, so this only drops the account's resting connections before
/// handing back a fresh binding. The one that failed is already gone, and the others rested
/// through whatever killed it, so the retry is given a connection dialled for it rather than
/// another from the same shelf.
fn make_imap_redial(imap: Arc<LiveImapAccount>, mailbox: MailboxId) -> Redial {
    Box::new(move || {
        imap.invalidate();
        let provider: Arc<dyn Provider> = Arc::new(imap.provider(mailbox.clone()));
        Box::pin(async move { Ok(provider) })
    })
}

/// Opens one IMAP `mailbox` of `account` through its `connections`: for a folder the account
/// listed after it was dialled, and for a folder opened before any pass has synced it. Dials
/// nothing when the account is already connected.
///
/// # Errors
///
/// Returns [`AccountError`] if `mailbox` is not a valid id, or the account had to be connected
/// and the connection or login failed.
pub async fn connect_imap_mailbox(
    connections: &ImapConnections,
    account: &AccountConfig,
    mailbox: &str,
    mime_wrapper: Option<Arc<dyn MimeWrapper>>,
) -> Result<Box<dyn Provider>, AccountError> {
    let mailbox = mailbox_id(mailbox)?;
    let imap = connections
        .current_or_connect(account, mime_wrapper)
        .await?;
    Ok(bind(&imap, mailbox))
}

/// Connects `account` afresh through its `connections` and binds **every folder** the account
/// lists that [`pass_syncs`], the INBOX first. An account pass syncs what is bound, so a folder
/// left out would be in no pass, and its mail would arrive only when someone opened it. Every
/// provider draws on the account's connections, so binding a folder opens no socket.
///
/// The INBOX is bound by its reserved name and the row the server lists for it is skipped, so it
/// is bound once however the server spells it. A folder the account lists later is bound by the
/// pass that first sees it, through [`connect_imap_mailbox`].
///
/// # Errors
///
/// Returns [`AccountError`] if the connection or login fails, the account's credential cannot be
/// resolved (the account stores no password), or the folder list cannot be fetched.
pub async fn connect_mail_providers(
    connections: &ImapConnections,
    account: &AccountConfig,
    account_id: &AccountId,
    mime_wrapper: Option<Arc<dyn MimeWrapper>>,
) -> Result<Vec<Box<dyn Provider>>, AccountError> {
    let imap = connections.connect(account, mime_wrapper).await?;
    let inbox_id = mailbox_id("INBOX")?;
    let inbox = imap.provider(inbox_id.clone());

    let listing = inbox
        .sync_mailboxes(account_id, None)
        .await
        .map_err(|err| AccountError::MailboxList(err.to_string()))?;
    let folders = match listing.update {
        SyncUpdate::Snapshot { objects, .. } => objects,
        SyncUpdate::Delta { changed, .. } => changed,
    };
    let others = folders
        .into_iter()
        .filter(|mailbox| mailbox.role != Some(MailboxRole::Inbox) && pass_syncs(mailbox))
        .map(|mailbox| mailbox.id);
    Ok(std::iter::once(inbox_id)
        .chain(others)
        .map(|mailbox| bind(&imap, mailbox))
        .collect())
}

#[cfg(test)]
#[path = "imap_tests.rs"]
mod tests;
