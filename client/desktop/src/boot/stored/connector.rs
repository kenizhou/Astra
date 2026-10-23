// SPDX-License-Identifier: GPL-3.0-only
//! The on-demand folder binder: the host's implementation of the app's
//! [`MailboxConnector`] seam over the dial table, called back into for a
//! late bind (a folder the account listed after the boot dial, or one
//! opened before any pass synced it). Split from `stored` for the file
//! cap.

use std::sync::Arc;

use async_trait::async_trait;
use engine_api::AccountId;

use astra_app::MailboxConnector;

use super::SharedCryptoStore;
use crate::{dial_table::AccountDials, HostProvider};

/// Binds a folder of a registered account on demand: the host's
/// implementation of the app's [`MailboxConnector`] seam, over the dial
/// table.
pub(crate) struct DialConnector {
    /// The registered accounts.
    dials: Arc<AccountDials>,
    /// The crypto store the IMAP arm's wrapper is built from (a late bind
    /// installs the same wrapper the boot dial did).
    crypto_store: SharedCryptoStore,
}

impl DialConnector {
    /// The binder over one dial table and the boot's crypto store.
    pub(crate) fn new(dials: Arc<AccountDials>, crypto_store: SharedCryptoStore) -> Self {
        Self {
            dials,
            crypto_store,
        }
    }
}

#[async_trait]
impl MailboxConnector<HostProvider> for DialConnector {
    async fn connect_folder(&self, account: &AccountId, mailbox_key: &str) -> Option<HostProvider> {
        let (config, connections) = self.dials.of(account.as_str())?;
        if let Some(jmap) = config.jmap.as_ref() {
            // One provider serves the whole JMAP account: the binder
            // reconnects that account-wide provider, whatever the
            // mailbox key names, and never mints a per-folder one. The
            // key is deliberately unread; a grant account dials over a
            // source minted here whose sink is none on purpose (a late
            // bind only reads, and the boot's sink-backed source owns
            // the rotations, exactly the graph arm's rule).
            let tls = astra_account::account_tls(&config).ok()?;
            let provider = match jmap.oauth.as_ref() {
                Some(_) => {
                    let source = astra_account::jmap_token_source(
                        jmap,
                        account.clone(),
                        None,
                        astra_account::CredentialOrigin::Stored,
                    )
                    .ok()?;
                    Box::new(
                        astra_account::dial_jmap_oauth_account(&config, &tls, &source)
                            .await
                            .ok()?,
                    ) as HostProvider
                }
                None => Box::new(astra_account::dial_jmap_account(&config, &tls).await.ok()?)
                    as HostProvider,
            };
            return Some(provider);
        }
        if let Some(graph) = config.graph.as_ref() {
            // A Graph account binds folders over its stored grant: a
            // source minted from the document through the same seam the
            // completion uses. The sink is none here on purpose: a late
            // bind only reads, and the boot's sink-backed source owns
            // the rotations, so this source never writes a document.
            let source = crate::oauth::graph_add::production_source()(
                graph.provider_config(),
                account.clone(),
                graph.refresh_token.expose().to_owned(),
                None,
            )
            .ok()?;
            return astra_account::connect_graph_folder(
                source,
                mailbox_key,
                None,
                &astra_account::account_tls(&config).ok()?,
            )
            .map(|provider| provider as HostProvider)
            .ok();
        }
        if let Some(google) = config.google.as_ref() {
            // A Google account binds folders over its stored grant: a
            // source minted from the document through the same
            // constructor the completion uses. The sink is none here on
            // purpose: a late bind only reads, and the boot's
            // sink-backed source owns the rotations, so this source never
            // writes a document. Gmail is account-global, so any mailbox
            // key binds the one provider.
            let source = astra_account::google_token_source(
                google,
                account.clone(),
                None,
                astra_account::CredentialOrigin::FreshSignIn,
            )
            .ok()?;
            let tls = astra_account::account_tls(&config).ok()?;
            let provider = astra_account::connect_google_folder(source, None, &tls) as HostProvider;
            return Some(provider);
        }
        let wrapper =
            crate::crypto_wrapper::mime_wrapper_for_config(&self.crypto_store, &config, account);
        astra_account::connect_imap_mailbox(&connections, &config, mailbox_key, wrapper)
            .await
            .ok()
    }
}
