// SPDX-License-Identifier: GPL-3.0-only
//! The outgoing name per account: the one value every send's From line
//! carries, adopted from the provider where it already knows one, set by
//! the person where it does not, and pushed back best-effort where the
//! provider's copy is the account holder's own.
//!
//! The name lives in the persisted preferences, beside the account, rather
//! than in the engine even where a provider keeps its own copy, because
//! IMAP has no server-side notion at all and two owners of one header is
//! the state to avoid: the stored copy is what reaches the wire, and the
//! provider's copy is a courtesy to the account's other clients. An entry
//! in the map (the empty string among them, the deliberate bare address) is
//! the ask-once latch: an account with a row there has answered the name
//! question, adopted or typed, and no account is asked twice.
//!
//! The decisions the module owns, recorded once:
//!
//! - **The ask is answered by adoption, not skipped by it.** Where the
//!   provider's identities answer a name for the account's own address, the
//!   join adopts it (a read-only name too: it is the one every other client
//!   of that mailbox shows), which spends the ask; where they answer
//!   nothing, the ask stands for the wizard or the accounts page to draw.
//! - **The set stores first and pushes second.** The stored value is the
//!   From line's source, so it moves before any server is asked and a
//!   provider that refuses the write leaves the person with the name they
//!   asked for, plus the refusal sentence on the row. No push is attempted
//!   where the provider's copy is not the account holder's to change, which
//!   is not a refusal and crosses no error.
//! - **One bound, one injection rule.** A name longer than the engine's
//!   cap, or carrying a control character, is refused at the setting with a
//!   sentence that never echoes the offending bytes: the same two rules the
//!   engine applies to the provider write, so the stored value can never be
//!   a header the engine would refuse to send.

use engine_api::{
    addresses_match, AccountId, EmailAddress, IdentityControls, Provider, SenderIdentity,
};

use astra_viewmodel::SettingsError;

use crate::App;

/// The longest sender name the app stores or pushes: the engine's own bound
/// for the provider write (its identities facade), mirrored here because the
/// engine keeps the constant private to that module. A name past it is
/// refused at the setting, never trimmed to fit.
pub(crate) const MAX_SENDER_NAME: usize = 128;

/// The sender-name fields the settings row carries for one account,
/// projected from the stored name beside the address the row falls back to.
pub(crate) struct SenderNameRow {
    /// The name the account's row shows: the stored name, or the address
    /// when the account sends as the bare address.
    pub(crate) display_name: String,
    /// Whether the account still needs its name asked for once.
    pub(crate) needs_sender_name: bool,
}

impl<P: Provider> App<P> {
    /// The join's identity half, run as the slow join path starts: read the
    /// provider's own identities and adopt the name it holds for the
    /// account's address, where one answers. Best-effort by design: a
    /// provider that is slow, unreachable, or holding nothing leaves the
    /// ask standing and nothing else waits on this. The adoption is local
    /// only, because the name came from the provider whose copy a push
    /// would write.
    pub(crate) async fn adopt_sender_name(&self, account: &AccountId) {
        let Some(handle) = self.account_handle(account).await else {
            return;
        };
        let Some(provider) = handle.providers.iter().find(|provider| {
            provider
                .connection_info()
                .capabilities
                .sender_identities()
                .is_some()
        }) else {
            return;
        };
        let identity = self
            .matching_identity(provider, &handle.identity.email, account)
            .await;
        let Some(name) = identity.and_then(|identity| identity.address.name) else {
            return;
        };
        if sender_name_refusal(&name).is_some() {
            let at = self.account_ordinal(account).await;
            log::info!("sender name: a{at}'s provider holds a name that cannot go in a header");
            return;
        }
        if self.store_sender_name(account, &name) {
            self.rebuild_settings().await;
        }
    }

    /// Whether the account still needs its name asked for once: the wizard's
    /// and the accounts page's ask-state. True until a value stands stored,
    /// so a provider whose name was adopted and a person who answered the
    /// bare address have both answered it.
    #[must_use]
    pub fn needs_sender_name(&self, account: &AccountId) -> bool {
        self.stored_sender_name(account).is_none()
    }

    /// The settings family's sender-name arm: refuse a name that cannot go
    /// in a header, store the value first (the stored copy is the From
    /// line's source, whatever the provider does), then push the provider's
    /// copy where the account holder owns it, best-effort. A re-assert of
    /// the standing value changes nothing and asks nothing.
    pub(crate) async fn set_sender_name(&self, account: AccountId, name: String) {
        if let Some(sentence) = sender_name_refusal(&name) {
            self.note_name_outcome(&account, Some(sentence));
            self.rebuild_settings().await;
            return;
        }
        let mut changed = false;
        let save_failed = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .set_setting(|prefs| {
                changed = prefs.set_account_sender_name(&account, name.clone());
            });
        if !changed {
            return;
        }
        let refusal = self.push_sender_name(&account, &name).await;
        self.note_name_outcome(&account, refusal);
        self.settings_state
            .lock()
            .expect("settings mutex poisoned")
            .error = save_failed.then_some(SettingsError::PersistFailed);
        self.rebuild_settings().await;
    }

    /// The From line's source: the account's address carrying the stored
    /// name, or the bare address when none stands. The one resolver, so no
    /// send path can put a bare address on the wire while the account rows
    /// show a name.
    pub(crate) fn send_identity(&self, handle: &crate::accounts::Account<P>) -> EmailAddress {
        match self.stored_sender_name(&handle.id) {
            Some(name) if !name.is_empty() => {
                EmailAddress::named(name, handle.identity.email.clone())
            }
            _ => handle.identity.clone(),
        }
    }

    /// The sender-name fields the settings row carries, beside the address
    /// the row falls back to.
    pub(crate) fn sender_name_row(&self, account: &AccountId, address: &str) -> SenderNameRow {
        let stored = self.stored_sender_name(account);
        SenderNameRow {
            display_name: stored
                .as_deref()
                .filter(|name| !name.is_empty())
                .map_or_else(|| address.to_owned(), str::to_owned),
            needs_sender_name: stored.is_none(),
        }
    }

    /// The stored sender name, `None` until a value (even the deliberate
    /// empty one) stands for the account.
    fn stored_sender_name(&self, account: &AccountId) -> Option<String> {
        self.pane
            .lock()
            .expect("preferences mutex poisoned")
            .prefs
            .sender_names
            .get(account)
            .cloned()
    }

    /// Stores the account's sender name and persists it. Returns whether
    /// anything changed, so re-asserting the standing value costs no disk
    /// write and no republish.
    fn store_sender_name(&self, account: &AccountId, name: &str) -> bool {
        let mut changed = false;
        self.pane
            .lock()
            .expect("preferences mutex poisoned")
            .set_setting(|prefs| {
                changed = prefs.set_account_sender_name(account, name.to_owned());
            });
        changed
    }

    /// Pushes the stored name to the provider whose copy the account holder
    /// owns, best-effort: the sentence of a refusal, or `None` when the
    /// push landed or was never this provider's to take.
    async fn push_sender_name(&self, account: &AccountId, name: &str) -> Option<String> {
        let handle = self.account_handle(account).await?;
        let provider = handle.providers.iter().find(|provider| {
            provider.connection_info().capabilities.sender_identities()
                == Some(IdentityControls::Writable)
        })?;
        let identity = self
            .matching_identity(provider, &handle.identity.email, account)
            .await?;
        let at = self.account_ordinal(account).await;
        match self
            .engine
            .set_sender_name(provider, account, &identity.id, name)
            .await
        {
            Ok(()) => {
                log::info!("sender name: a{at}'s server copy now holds the name that was set");
                None
            }
            Err(err) => {
                let sentence = err.to_string();
                log::warn!(
                    "sender name: a{at} kept the local name; the server refused its copy \
                     ({sentence})"
                );
                Some(sentence)
            }
        }
    }

    /// The provider's identity for the account's own address, matched on the
    /// address and never taken as the first entry, because the order a
    /// provider returns is its own. `None` when the provider cannot be asked
    /// or answers no identity for the address.
    async fn matching_identity(
        &self,
        provider: &P,
        email: &str,
        account: &AccountId,
    ) -> Option<SenderIdentity> {
        let identities = match self.engine.sender_identities(provider, account).await {
            Ok(identities) => identities,
            Err(err) => {
                let at = self.account_ordinal(account).await;
                log::info!("sender name: a{at}'s provider did not answer its identities ({err})");
                return None;
            }
        };
        identities
            .into_iter()
            .find(|identity| addresses_match(&identity.address.email, email))
    }

    /// Records the name write's outcome on the account's row: the sentence
    /// of a push that was refused (the local name stands), or the clean
    /// state of one that landed or was never attempted.
    fn note_name_outcome(&self, account: &AccountId, sentence: Option<String>) {
        let mut state = self.settings_state.lock().expect("settings mutex poisoned");
        match sentence {
            Some(sentence) => {
                state.name_errors.insert(account.clone(), sentence);
            }
            None => {
                state.name_errors.remove(account);
            }
        }
    }
}

/// How the account's outgoing name is governed, off the providers' identity
/// capability: the first provider that has identities at all states it, and
/// a provider with none leaves the name the app's own (the IMAP shape).
pub(crate) fn sender_name_control<P: Provider>(
    providers: &[P],
) -> astra_viewmodel::SenderNameControl {
    providers
        .iter()
        .find_map(|provider| provider.connection_info().capabilities.sender_identities())
        .map_or(
            astra_viewmodel::SenderNameControl::Local,
            |controls| match controls {
                IdentityControls::Writable => astra_viewmodel::SenderNameControl::Writable,
                IdentityControls::ReadOnly => astra_viewmodel::SenderNameControl::ReadOnly,
            },
        )
}

/// The setting's own refusal: a name that cannot go in a `From` header,
/// with a sentence that never echoes the offending bytes. The same two
/// rules the engine applies to the provider write, applied here so the
/// stored value can never be a header the engine would refuse to send.
fn sender_name_refusal(name: &str) -> Option<String> {
    if name.chars().count() > MAX_SENDER_NAME {
        return Some(format!(
            "the name is longer than {MAX_SENDER_NAME} characters"
        ));
    }
    if name.chars().any(char::is_control) {
        return Some("the name contains a control character".to_owned());
    }
    None
}

#[cfg(test)]
#[path = "sender_names_tests.rs"]
mod tests;
