// SPDX-License-Identifier: GPL-3.0-only
//! The account-info seam and its write halves: the host-installed port the
//! settings surface reads the account configs through, and the two verbs
//! that drive it back.
//!
//! The decisions the write halves own, recorded once:
//!
//! - **The re-dial is the host's.** [`HostAccountOps::store_dav`] merges the
//!   editor's input over the stored section host-side (an absent password
//!   keeps the stored one, because the credential never crosses back),
//!   persists the config, and dials, exactly as the boot's folder connector
//!   dials: the secure store and the TLS policy live with the host. The app
//!   installs what comes back: the calendar provider replaces the account's
//!   calendar leg wholesale (the providers that carry no calendar
//!   capability stay, the dialled leg appends after them, the append-after
//!   rule the boot dials), and the contacts leg installs through
//!   [`install_contacts`].
//! - **The swap runs under the account set's write lock, and a busy handle
//!   answers busy.** The lock excludes new readers, so the strong count of
//!   an entry is stable under it: a count above one means a dispatch still
//!   holds the old account across an engine call, the swap refuses with a
//!   sentence on the row, and the old legs keep standing until the save is
//!   made again.
//! - **A failed write keeps everything that stood.** The seam's `Err` lands
//!   on the account's row as `dav_error` (or on the snapshot's error channel
//!   for a removal); nothing installed, stored, or torn down moves.
//! - **Tearing a leg down forgets its store domain.** The unified views read
//!   the engine's store, not the providers, so a cleared dav section or a
//!   removed account also forgets the domains its legs filled (the calendar
//!   and contacts domains for a clear; every domain, the mail half's
//!   included, for a removal, whose mail half and legs go together).
//!   Forgetting is best effort beyond the config: a store failure is logged
//!   and the teardown continues, because the row the surface renders is
//!   gone the moment the account entry is.
//!
//! [`install_contacts`]: crate::App::install_contacts

use async_trait::async_trait;
use engine_api::{AccountId, ContactsProvider, Provider, SearchDomain};
use std::sync::Arc;

use crate::protocol::DavSectionInput;
use crate::{App, Scope};

/// One configured account as the host's dial table describes it: the facts
/// the settings rows render that the app's own account set cannot see.
#[derive(Debug, Clone)]
pub struct AccountInfo {
    /// The account the info names.
    pub id: AccountId,
    /// The account's kind: how its mail half dials.
    pub kind: astra_viewmodel::AccountKind,
    /// The stored dav section's echo, when the account carries one. The
    /// credential never crosses this seam; the echo answers whether one is
    /// stored.
    pub dav: Option<astra_viewmodel::DavSectionRow>,
}

/// The dialled legs a stored dav section re-dials into: the shapes the app
/// installs, matching the two halves the boot dials at assembly.
pub struct DavLegs<P> {
    /// The calendar leg, to append after the account's mail providers.
    pub caldav: Option<P>,
    /// The contacts leg, to install beside them.
    pub carddav: Option<Box<dyn ContactsProvider>>,
}

impl<P> core::fmt::Debug for DavLegs<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The legs hold live connections; which halves stand is the whole
        // safe description.
        f.debug_struct("DavLegs")
            .field("caldav", &self.caldav.is_some())
            .field("carddav", &self.carddav.is_some())
            .finish()
    }
}

/// The host's account store, installed at assembly beside the accounts it
/// describes: the read half answers what each stored account is, the write
/// halves persist an edited dav section (re-dialling it) and remove an
/// account's stored config and credential.
///
/// The seam exists because the app is generic over its mail provider and
/// cannot see the stored configs: the OS secure store, the dial table, and
/// the re-dial live with the host, exactly as the folder connector does. An
/// `Err` from either write half is a sentence the settings row shows; the
/// host keeps whatever stood stored and standing.
#[async_trait]
pub trait HostAccountOps<P>: Send + Sync {
    /// Every configured account's info. The app joins this against its own
    /// account set; an info naming an account the app does not hold renders
    /// no row.
    async fn account_infos(&self) -> Vec<AccountInfo>;

    /// Merges `section` over the account's stored dav section host-side
    /// (an absent password keeps the stored one), persists the config, and
    /// re-dials, answering the dialled legs. `None` drops the section and
    /// answers empty legs.
    ///
    /// # Errors
    ///
    /// The sentence the settings row shows when the merged section names no
    /// origin to dial, or the persist or the re-dial failed. Nothing changed
    /// stored or standing.
    async fn store_dav(
        &self,
        account: &AccountId,
        section: Option<DavSectionInput>,
    ) -> Result<DavLegs<P>, String>;

    /// Drops the account's stored config and its credential from the host's
    /// secure store.
    ///
    /// # Errors
    ///
    /// The sentence the settings surface shows when the stored config could
    /// not be removed; the account keeps everything it had.
    async fn remove_account(&self, account: &AccountId) -> Result<(), String>;
}

impl<P: Provider> App<P> {
    /// The dav editor's save: the seam persists the section and re-dials it,
    /// and the app installs what comes back. A `None` clears: the legs tear
    /// down and the config drops the section.
    pub(crate) async fn set_dav(&self, account: AccountId, section: Option<DavSectionInput>) {
        let Some(ops) = self.account_ops() else {
            self.answer_dav_failure(
                &account,
                "this build has no host account store to save through",
            )
            .await;
            return;
        };
        if self.account_handle(&account).await.is_none() {
            log::warn!("settings: the account whose dav section was saved is not running");
            self.rebuild_settings().await;
            return;
        }
        let clearing = section.is_none();
        match ops.store_dav(&account, section).await {
            Err(sentence) => {
                let at = self.account_ordinal(&account).await;
                log::warn!("settings[a{at}]: the dav section could not be saved: {sentence}");
                self.answer_dav_failure(&account, &sentence).await;
            }
            Ok(legs) => {
                if let Err(sentence) = self.install_dav_legs(&account, legs).await {
                    self.answer_dav_failure(&account, &sentence).await;
                    return;
                }
                if clearing {
                    self.forget_domains(
                        &account,
                        &[SearchDomain::Calendar, SearchDomain::Contacts],
                    )
                    .await;
                    self.clear_account_calendar_state(&account);
                }
                self.answer_dav_success(&account);
                self.rebuild_calendar().await;
                self.rebuild_people().await;
                self.rebuild_settings().await;
            }
        }
    }

    /// Installs dialled `legs` on `account`: the calendar provider replaces
    /// the account's calendar leg, the contacts leg installs beside the mail
    /// providers. `Err` is the busy sentence the row shows; whatever stood,
    /// stands.
    async fn install_dav_legs(&self, account: &AccountId, legs: DavLegs<P>) -> Result<(), String> {
        let mut accounts = self.accounts.write().await;
        let Some(at) = accounts.iter().position(|held| held.id == *account) else {
            return Ok(());
        };
        // Under the write lock no new reader can clone the entry, so a
        // strong count above one is a dispatch still holding the old
        // account across an engine call: the swap would tear a provider out
        // from under a live read, so it refuses and the save is made again.
        if Arc::strong_count(&accounts[at]) > 1 {
            return Err("the account is mid-flight; save it again once it settles".to_owned());
        }
        let held = accounts.remove(at);
        let mut owned =
            Arc::try_unwrap(held).expect("the strong count of one unwraps under the write lock");
        owned
            .providers
            .retain(|provider| !provider.connection_info().capabilities.calendars());
        owned.providers.extend(legs.caldav);
        accounts.insert(at, Arc::new(owned));
        match legs.carddav {
            Some(carddav) => self.install_contacts(account, carddav),
            None => self.remove_contacts(account),
        }
        Ok(())
    }

    /// The removal verb: the seam drops the stored config and the
    /// credential, and the app tears its own halves down with them. Nothing
    /// goes when the seam refuses.
    pub(crate) async fn remove_account(&self, account: AccountId) {
        let Some(ops) = self.account_ops() else {
            self.answer_removal_failure().await;
            return;
        };
        if let Err(sentence) = ops.remove_account(&account).await {
            let at = self.account_ordinal(&account).await;
            log::warn!("settings[a{at}]: the account could not be removed: {sentence}");
            self.answer_removal_failure().await;
            return;
        }
        self.accounts
            .write()
            .await
            .retain(|held| held.id != account);
        self.remove_contacts(&account);
        self.clear_people_write(&account);
        self.clear_account_calendar_state(&account);
        self.forget_domains(
            &account,
            &[
                SearchDomain::Mail,
                SearchDomain::Calendar,
                SearchDomain::Contacts,
            ],
        )
        .await;
        let removed = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .remove_account(account.as_str());
        if removed {
            log::info!("settings: an account's stored preferences went with it");
        }
        // The selection that pointed inside the account returns to the
        // unified view; the preferences above already cleared the persisted
        // half, so this moves only the running scope.
        let scope_names_account = match &*self.scope.lock().expect("scope mutex poisoned") {
            Scope::AllInboxes => false,
            Scope::Account(held)
            | Scope::Folder(crate::reference::FolderRef { account: held, .. }) => held == &account,
        };
        if scope_names_account {
            *self.scope.lock().expect("scope mutex poisoned") = Scope::AllInboxes;
        }
        self.reset_window();
        self.bump_generation();
        self.rebuild_snapshot().await;
        self.rebuild_calendar().await;
        self.rebuild_people().await;
        self.answer(None).await;
    }

    /// Records a dav failure on the account's row and republishes.
    async fn answer_dav_failure(&self, account: &AccountId, sentence: &str) {
        self.settings_state
            .lock()
            .expect("settings mutex poisoned")
            .dav_errors
            .insert(account.clone(), sentence.to_owned());
        self.rebuild_settings().await;
    }

    /// Clears the account's dav failure: the state of a save that landed,
    /// recorded before the rebuilds that show it.
    fn answer_dav_success(&self, account: &AccountId) {
        self.settings_state
            .lock()
            .expect("settings mutex poisoned")
            .dav_errors
            .remove(account);
    }

    /// Records the removal's failure on the snapshot's error channel and
    /// republishes.
    async fn answer_removal_failure(&self) {
        self.settings_state
            .lock()
            .expect("settings mutex poisoned")
            .error = Some(astra_viewmodel::SettingsError::RemoveFailed);
        self.rebuild_settings().await;
    }

    /// Forgets the account's store `domains`, best effort: a store failure
    /// is logged and the teardown continues.
    async fn forget_domains(&self, account: &AccountId, domains: &[SearchDomain]) {
        for domain in domains {
            if let Err(err) = self.engine.forget_account_domain(account, *domain).await {
                log::warn!(
                    "settings: an account's {domain:?} domain could not be forgotten: {err}"
                );
            }
        }
    }
}
