// SPDX-License-Identifier: GPL-3.0-only
//! The host's wrapper factory (ECS): builds the engine MIME wrapper one IMAP
//! account's dial installs, from the stored config's `[crypto]` section and
//! the shared crypto store. An account without a method gets `None`: the
//! zero-cost default, sends untouched. The wrapper itself
//! (`astra_crypto_store::mime_wrapper_for`) is fail-closed: a store failure
//! yields a wrapper that refuses every crypto send, never a plaintext
//! fallback. Secrets stay in-process: the wrapper never crosses an IPC
//! boundary.

use std::sync::Arc;

use engine_api::AccountId;

use astra_crypto_core::CryptoMethod;
use astra_crypto_store::CryptoStore;

/// The wrapper one account's dial installs, read from its stored config:
/// the `[crypto]` method and the account's own address (the IMAP username),
/// or `None` when no method stands. The caller dials IMAP only: a config
/// without an `[imap]` section answers `None` here too (the settings write
/// is the fail-closed guard that keeps such a config from standing).
pub(crate) fn mime_wrapper_for_config(
    store: &Arc<CryptoStore>,
    config: &astra_account::AccountConfig,
    account_id: &AccountId,
) -> Option<Arc<dyn engine_api::MimeWrapper>> {
    let method = config.crypto.as_ref().map_or(CryptoMethod::None, |crypto| {
        CryptoMethod::from_db_str(&crypto.method)
    });
    if method == CryptoMethod::None {
        return None;
    }
    let email = config.imap.as_ref().map(|imap| imap.username.clone())?;
    astra_crypto_store::mime_wrapper_for(store, account_id.as_str(), &email, method)
}

/// The add-account dial both host shapes (the shell's command, the headless
/// slice) drive: a fresh connection pool, the stored config's wrapper
/// installed exactly as the boot's dials install it.
///
/// # Errors
///
/// The dial's own [`astra_account::AccountError`]: the connection or the
/// first login failed.
pub(crate) fn add_account_dial(
    crypto_store: &Arc<CryptoStore>,
    runtime: &crate::AppRuntime,
    config: &astra_account::AccountConfig,
    account_id: &AccountId,
) -> Result<
    (
        Vec<crate::HostProvider>,
        Arc<astra_account::ImapConnections>,
    ),
    astra_account::AccountError,
> {
    let connections = astra_account::ImapConnections::new();
    let wrapper = mime_wrapper_for_config(crypto_store, config, account_id);
    let providers = runtime.block_on(|| {
        astra_account::connect_mail_providers(&connections, config, account_id, wrapper)
    })?;
    Ok((providers, connections))
}

#[cfg(test)]
#[path = "crypto_wrapper_tests.rs"]
mod tests;
