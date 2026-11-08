// SPDX-License-Identifier: GPL-3.0-only
//! The host's account store: the settings seam's implementation over the
//! dial table and the credential store. The read half answers one info
//! per registered config; the write halves persist an edited dav section
//! (re-dialling it) and remove an account's stored config and credential.
//!
//! The decisions this module owns, recorded once:
//!
//! - **The merge rule.** Each field the editor's input states replaces
//!   what stood; each field it leaves absent keeps it, the omission
//!   discipline [`DavSectionInput`]'s own doc states (an edit of the
//!   username alone crosses no key for the origins). Blank text states a
//!   cleared origin, because the config layer's own accessors read blank
//!   as no server; a blank or absent username keeps the stored login,
//!   because a section with no login dials nothing and the row always
//!   echoes the standing login back. The password keeps whenever the
//!   input states none, because the echo never carries it: the editor
//!   sends absent unless a new one is typed, and a stated password
//!   replaces the stored one verbatim.
//! - **The dial runs before the persist.** The seam's contract keeps
//!   everything that stood on an `Err`, so a failed re-dial must leave
//!   the stored section exactly as it was: the merged config dials
//!   first, and only a dial that answered legs persists. A persist the
//!   credential store refuses likewise stores nothing, and the dialled
//!   legs go unanswered with it.
//! - **Every registered account names its own kind.** A stored config
//!   names an `[imap]` section, an OAuth family's grant (`[graph]` or
//!   `[google]`), or a `[jmap]` section, password or grant; a grant
//!   answers its family's kind (the `microsoft` kind for Graph, the
//!   `google` kind for Google), a jmap section the `jmap` kind, and the
//!   address is the section's own email beside the standing grant
//!   refusal the boot dial or the completion recorded. No dav-only
//!   config shape exists for this host to register; that kind stays
//!   unreachable until one lands.
//! - **The removal's halves go in order.** The credential entry, the
//!   durable half, deletes first: a refusal answers `Err` with nothing
//!   moved anywhere. The dial table's entry and its index line go after;
//!   an index write that fails there leaves a stale id the next boot's
//!   own load tolerates by design (warn and skip), so the removal still
//!   answers `Ok` rather than pretending an account whose credential is
//!   gone still stands.
//!
//! [`DavSectionInput`]: astra_app::DavSectionInput

use std::{future::Future, sync::Arc};

use async_trait::async_trait;
use engine_api::AccountId;

use astra_app::{AccountInfo, DavLegs, DavSectionInput, HostAccountOps};
use astra_viewmodel::{AccountKind, DavSectionRow};

use crate::app_state::HostProvider;
use crate::credential_store::AccountCredentialStore;
use crate::dial_table::AccountDials;

/// The sentence a merged section that names no origin answers: nothing is
/// diallable, and a stored section with no origin would refuse to load at
/// the next boot.
const NO_ORIGIN: &str = "the merged section names no origin to dial";

/// The sentence a verb naming an account the dial table does not hold
/// answers.
const NOT_REGISTERED: &str = "the account has no stored config on this host";

/// The host's [`HostAccountOps`] over the dial table, the credential
/// store, and the standing grant refusals: the boot installs one at
/// assembly, and the settings surface reads the account configs and
/// writes them back through it. All three stores the halves move are the
/// host's own, exactly as the boot's dial and the account-add flow use
/// them.
pub(crate) struct HostAccountStore {
    /// The registered accounts: the configs the read half describes and
    /// the write halves edit.
    dials: Arc<AccountDials>,
    /// The secure store the config documents persist through and the
    /// removal erases from.
    credentials: Arc<dyn AccountCredentialStore>,
    /// The standing grant refusals the read half answers beside each
    /// config, and the removal clears an account's entry from.
    refusals: Arc<crate::oauth::grant_refusals::GrantRefusals>,
    /// The crypto store the crypto-method write's re-dial builds the new
    /// method's MIME wrapper from.
    crypto_store: Arc<astra_crypto_store::CryptoStore>,
}

impl HostAccountStore {
    /// A store over the boot's dial table, credential store, refusal map,
    /// and crypto store.
    pub(crate) fn new(
        dials: Arc<AccountDials>,
        credentials: Arc<dyn AccountCredentialStore>,
        refusals: Arc<crate::oauth::grant_refusals::GrantRefusals>,
        crypto_store: Arc<astra_crypto_store::CryptoStore>,
    ) -> Self {
        Self {
            dials,
            credentials,
            refusals,
            crypto_store,
        }
    }

    /// The crypto-method write's own half under an injected `dial`, which
    /// receives the merged config, the account's connections, and the wrapper
    /// the new method builds (or `None` once cleared) and answers the
    /// re-dialled mail providers; the real verb passes the boot's own
    /// connect, the tests pass stands-ins so no test touches a network. The
    /// order the contract rests on: merge, refuse, RE-DIAL, persist, swap in
    ///: the write takes effect with the next send because the account's
    /// connections were rebuilt against it, and a dial that fails leaves the
    /// old document stored. Fail-closed at the boundary the engine's wrap
    /// seam cannot carry: a method on an account without both dial halves (a
    /// Graph/Google/JMAP account, or IMAP without SMTP) is refused outright,
    /// because the write would otherwise stand while sends went out plainly.
    pub(crate) async fn store_crypto_method_with<D, Fut>(
        &self,
        account: &AccountId,
        method: &str,
        dial: D,
    ) -> Result<Option<Vec<HostProvider>>, String>
    where
        D: FnOnce(
            astra_account::AccountConfig,
            Arc<astra_account::ImapConnections>,
            Option<Arc<dyn engine_api::MimeWrapper>>,
        ) -> Fut,
        Fut: Future<Output = Result<Vec<HostProvider>, astra_account::AccountError>>,
    {
        let id = account.as_str().to_owned();
        let (mut config, connections) = self
            .dials
            .of(&id)
            .ok_or_else(|| NOT_REGISTERED.to_owned())?;
        config.crypto = match method {
            "none" => None,
            other => Some(astra_account::CryptoAccount {
                method: other.to_owned(),
            }),
        };
        // The fail-closed guard: no seam, no write.
        if config.crypto.is_some() && (config.imap.is_none() || config.smtp.is_none()) {
            return Err("encryption needs an IMAP/SMTP account".to_owned());
        }
        // An account with no IMAP dial (a grant account clearing a legacy
        // method) persists and answers no re-dial.
        if config.imap.is_none() {
            let document = config.to_toml().map_err(|err| err.to_string())?;
            self.credentials
                .persist(&id, &document)
                .map_err(|err| err.to_string())?;
            self.dials.replace_config(&id, config);
            return Ok(None);
        }
        let wrapper =
            crate::crypto_wrapper::mime_wrapper_for_config(&self.crypto_store, &config, account);
        let providers = dial(config.clone(), connections, wrapper)
            .await
            .map_err(|err| err.to_string())?;
        let document = config.to_toml().map_err(|err| err.to_string())?;
        self.credentials
            .persist(&id, &document)
            .map_err(|err| err.to_string())?;
        self.dials.replace_config(&id, config);
        Ok(Some(providers))
    }

    /// The save's own half under an injected `dial`, which receives the
    /// merged config and answers the providers it dials; the real verb
    /// passes the boot's own connect, the tests pass stands-ins so no
    /// test touches a network. The order the contract rests on: merge,
    /// refuse, dial, persist, swap in.
    async fn store_dav_with<D, Fut>(
        &self,
        account: &AccountId,
        section: Option<DavSectionInput>,
        dial: D,
    ) -> Result<DavLegs<HostProvider>, String>
    where
        D: FnOnce(astra_account::AccountConfig) -> Fut,
        Fut: Future<Output = Result<astra_account::DavProviders, astra_account::AccountError>>,
    {
        let id = account.as_str().to_owned();
        let (mut config, _connections) = self
            .dials
            .of(&id)
            .ok_or_else(|| NOT_REGISTERED.to_owned())?;
        match section {
            None => config.dav = None,
            Some(input) => {
                let merged = merged_dav(config.dav.as_ref(), &input);
                if merged.caldav_origin().is_none() && merged.carddav_origin().is_none() {
                    return Err(NO_ORIGIN.to_owned());
                }
                config.dav = Some(merged);
            }
        }
        let providers = dial(config.clone()).await.map_err(|err| err.to_string())?;
        let document = config.to_toml().map_err(|err| err.to_string())?;
        self.credentials
            .persist(&id, &document)
            .map_err(|err| err.to_string())?;
        self.dials.replace_config(&id, config);
        Ok(DavLegs {
            caldav: providers.caldav,
            carddav: providers.carddav,
        })
    }
}

/// The merged section an editor input leaves standing over `base`: each
/// stated field replaces, each absent field keeps, the password keeping
/// whenever the input states none.
fn merged_dav(
    base: Option<&astra_account::DavAccount>,
    input: &DavSectionInput,
) -> astra_account::DavAccount {
    astra_account::DavAccount {
        caldav_url: match input.caldav_url.as_deref() {
            Some(url) => non_blank(Some(url)),
            None => base.and_then(|dav| dav.caldav_url.clone()),
        },
        carddav_url: match input.carddav_url.as_deref() {
            Some(url) => non_blank(Some(url)),
            None => base.and_then(|dav| dav.carddav_url.clone()),
        },
        username: match input.username.as_deref() {
            Some(name) if !name.trim().is_empty() => name.to_owned(),
            _ => base.map_or_else(String::new, |dav| dav.username.clone()),
        },
        password: match &input.password {
            Some(password) => Some(astra_account::Secret::new(password.clone())),
            None => base.and_then(|dav| dav.password.clone()),
        },
    }
}

/// The masked echo the settings row renders: the stored section minus the
/// credential, stating only that one is stored.
fn dav_echo(dav: &astra_account::DavAccount) -> DavSectionRow {
    DavSectionRow {
        caldav_url: non_blank(dav.caldav_url.as_deref()),
        carddav_url: non_blank(dav.carddav_url.as_deref()),
        username: dav.username.clone(),
        dav_password_set: dav.password.is_some(),
    }
}

/// `Some` for text that names something, trimmed; the config layer's own
/// blank discipline.
fn non_blank(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned())
}

#[async_trait]
impl HostAccountOps<HostProvider> for HostAccountStore {
    async fn account_infos(&self) -> Vec<AccountInfo> {
        let mut infos = Vec::new();
        for id in self.dials.ids() {
            let Some((config, _connections)) = self.dials.of(&id) else {
                continue;
            };
            let account_id = match config.account_id() {
                Ok(account_id) => account_id,
                Err(err) => {
                    log::warn!(
                        "settings: a registered account could not derive its id and renders no row: {err}"
                    );
                    continue;
                }
            };
            // A stored grant is an account addressed by the lookup the
            // sign-in named (the Microsoft family over Graph, the Google
            // family over its native APIs); a jmap section is its own
            // kind, password or grant, addressed by its own email; every
            // other stored config names an imap section whose login is
            // the address.
            let (kind, address) = match (
                config.graph.as_ref(),
                config.google.as_ref(),
                config.jmap.as_ref(),
            ) {
                (Some(graph), _, _) => (AccountKind::Microsoft, graph.email.clone()),
                (None, Some(google), _) => (AccountKind::Google, google.email.clone()),
                (None, None, Some(jmap)) => (AccountKind::Jmap, jmap.email.clone()),
                (None, None, None) => (
                    AccountKind::ImapSmtp,
                    config
                        .imap
                        .as_ref()
                        .map_or_else(String::new, |imap| imap.username.clone()),
                ),
            };
            infos.push(AccountInfo {
                id: account_id,
                kind,
                address,
                grant: self.refusals.of(&id),
                dav: config.dav.as_ref().map(dav_echo),
                crypto_method: config
                    .crypto
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), |crypto| crypto.method.clone()),
            });
        }
        infos
    }

    async fn store_dav(
        &self,
        account: &AccountId,
        section: Option<DavSectionInput>,
    ) -> Result<DavLegs<HostProvider>, String> {
        self.store_dav_with(account, section, |config| async move {
            astra_account::connect_dav_providers(&config).await
        })
        .await
    }

    async fn remove_account(&self, account: &AccountId) -> Result<(), String> {
        self.credentials
            .delete(account.as_str())
            .map_err(|err| err.to_string())?;
        if let Err(err) = self.dials.remove(account.as_str()) {
            log::warn!(
                "settings: an account's id stayed in the dial table's index after removal: {err}"
            );
        }
        // The account's row goes, so the refusal state it carried goes with
        // it: a later re-add of the same account starts clean.
        self.refusals.clear(account.as_str());
        Ok(())
    }

    async fn store_crypto_method(
        &self,
        account: &AccountId,
        method: &str,
    ) -> Result<Option<Vec<HostProvider>>, String> {
        self.store_crypto_method_with(account, method, |config, connections, wrapper| async move {
            astra_account::connect_mail_providers(&connections, &config, account, wrapper).await
        })
        .await
    }
}

#[cfg(test)]
#[path = "account_ops_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "account_ops_jmap_tests.rs"]
mod jmap_tests;
