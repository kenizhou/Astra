// SPDX-License-Identifier: GPL-3.0-only
//! The account watch wrapper: the neutral face the desktop's delivery loop
//! opens one account's push channel through. The IMAP arm stands an `IDLE`
//! watch on the account's INBOX behind the engine's headroom guard, the
//! JMAP arm opens the EventSource stream the stored section's server
//! advertises, and every other family answers `Poll`, the account's own
//! cadence carrying its delivery.

use engine_core::sync::JmapDataType;

use crate::{AccountConfig, AccountError, ImapConnections};

/// Re-exported so a host drives the session without depending on the
/// engine's provider crate directly.
pub use engine_provider::{Watch, WatchEvent};

/// How one account's delivery reaches the host: a standing watch session,
/// or the poll cadence.
pub enum WatchFace {
    /// A standing watch session: `Changed` runs the account's normal pass,
    /// `KeepAlive` is a heartbeat, and an error is the host's reconnect
    /// signal.
    Watch(Box<dyn Watch>),
    /// The account has no push face; its cadence carries the delivery.
    Poll,
}

impl core::fmt::Debug for WatchFace {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The session may hold connection state; the face alone is the safe
        // description.
        f.debug_struct("WatchFace")
            .field("watching", &matches!(self, WatchFace::Watch(_)))
            .finish()
    }
}

/// Opens the account's watch face over its dialled connections.
///
/// The choice is the server's own answer, never a guess: an IMAP server
/// that advertises no `IDLE`, a pool whose watch would take the last
/// connection syncing needs, and a JMAP session that advertises no
/// `eventSourceUrl` all answer [`WatchFace::Poll`], so the caller falls
/// back to the cadence rather than retrying a refusal. The Graph and
/// Google families answer `Poll` from the stored document alone, and so
/// does a JMAP account whose only credential is an OAuth grant: the
/// refreshing provider holds its token, and the stream has nothing of its
/// own to present.
///
/// # Errors
///
/// Returns [`AccountError`] when the account's connection itself fails:
/// the caller records the network failure and reconnects per its own
/// backoff.
pub async fn open_watch(
    connections: &ImapConnections,
    account: &AccountConfig,
) -> Result<WatchFace, AccountError> {
    if let Some(jmap) = account.jmap.as_ref() {
        // A password the section stores is a secret the stream presents
        // itself; a grant is not, so the account rides its cadence.
        let Some(credentials) = jmap.credentials() else {
            return Ok(WatchFace::Poll);
        };
        let tls = crate::account_tls(account)?;
        let id = account
            .account_id()
            .map_err(|err| AccountError::Jmap(err.to_string()))?;
        let config = provider_jmap::JmapConfig::new(jmap.base_url.clone(), credentials)
            .with_tls(tls)
            .with_retry(crate::throttle::account_retry(&id));
        let types = [JmapDataType::Email, JmapDataType::Mailbox];
        return match provider_jmap::JmapWatcher::connect(
            config,
            &types,
            provider_jmap::DEFAULT_EVENT_SOURCE_PING,
        )
        .await
        {
            Ok(watcher) => Ok(WatchFace::Watch(Box::new(watcher))),
            Err(err) if err.class() == engine_api::FailureClass::InvalidState => {
                Ok(WatchFace::Poll)
            }
            Err(err) if err.class() == engine_api::FailureClass::Authentication => {
                Err(AccountError::SigninRejected(err.to_string()))
            }
            Err(err) => Err(AccountError::Watch(err)),
        };
    }
    if account.imap.is_some() {
        return connections
            .watch_inbox(account, provider_imap::DEFAULT_IDLE_KEEPALIVE)
            .await;
    }
    Ok(WatchFace::Poll)
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
