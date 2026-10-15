// SPDX-License-Identifier: GPL-3.0-only
//! The settings half of the runtime: the [`SettingsIntent`] dispatch over
//! the persisted preferences and the host's account store, the snapshot a
//! rebuild projects, and the account-info seam the accounts page reads the
//! account configs through and writes them back over.
//!
//! The decisions this module owns, recorded once:
//!
//! - **The account-info seam** ([`HostAccountOps`], in [`accounts`]) is the
//!   [`install_contacts`] precedent applied to the whole account config: the
//!   app is generic over its mail provider and cannot see the stored
//!   configs, so the host installs one port at assembly and the settings
//!   surface drives it. The **read half** answers one [`AccountInfo`] per
//!   configured account (its kind, its address, its standing grant refusal
//!   state, and its dav section's masked echo); the
//!   rebuild joins that against the app's own account set, which stays the
//!   authority on what is configured, and the address and the legs flags
//!   come from the app's own facts (the identity, the providers'
//!   capabilities, the installed contacts legs) so they are freshly true the
//!   moment a re-dial installs anything. An account with no host info
//!   renders the mail-only shape (`imap_smtp`, no dav row); an info naming
//!   an account the app does not hold renders no row, because a config the
//!   app is not running is not an account the page can act on, **except**
//!   when the info carries a grant refusal state: an account whose grant
//!   the host reports dead never dialled, and the row it renders anyway is
//!   the only surface that can offer the re-grant. The **write halves** are
//!   [`SetDav`](SettingsIntent::SetDav) and
//!   [`RemoveAccount`](SettingsIntent::RemoveAccount); see [`accounts`].
//! - **One error channel, typed**: the snapshot's `error` names why the
//!   newest total write refused or failed
//!   ([`SettingsError`](astra_viewmodel::SettingsError)); the per-account
//!   `dav_error` on a row names why that account's dav editor last refused
//!   or failed, and the per-account `refresh_error` names why that
//!   account's last zone change could not rewrite its stored occurrences
//!   (the zone stands; a later zone change retries and clears it), the two
//!   failures that belong to an account rather than the block. All state
//!   the newest write, never the history.
//! - **The first day of the week lives web-side, carried by the settings
//!   block.** The grids lay their weeks out in the host's renderer, and the
//!   rows the calendar snapshot carries are the same either way, so the
//!   calendar snapshot gains no field of its own: the settings block is the
//!   one source, and the core's own consumption is the create path's WKST
//!   (a rule the editor writes that names no weekday starts its weeks on
//!   the setting's day).
//! - **The default calendar is the settings block's, additively.** The
//!   calendar snapshot's rows already carry the server's own default
//!   marking; the app-level pin travels beside them in the settings block,
//!   the one source the create dialog reads, so the two facts can never
//!   disagree by living on two surfaces.
//!
//! [`install_contacts`]: crate::App::install_contacts
//! [`accounts`]: self::accounts

mod accounts;

pub use accounts::{AccountInfo, DavLegs, HostAccountOps};

use std::{collections::BTreeMap, sync::Arc};

use astra_viewmodel::{AboutBlock, SettingsError, SettingsSnapshot};
use engine_api::{AccountId, Provider};

use crate::protocol::SettingsIntent;
use crate::{App, Preferences, ViewMode};

/// The settings view state: what a rebuild projects and the write arms move.
/// Held behind one mutex because the rebuild reads all of it and every write
/// arm moves exactly one field of it.
#[derive(Default)]
pub(crate) struct SettingsState {
    /// The per-account dav failure sentences: the editor's last refusal or
    /// failed re-dial, shown on the account's row. Absent for an account
    /// whose last dav write landed.
    pub(crate) dav_errors: BTreeMap<AccountId, String>,
    /// The per-account re-expansion failures: the accounts whose last zone
    /// change could not rewrite their stored occurrences, shown on the
    /// account's row. Absent for an account whose re-expansion landed.
    pub(crate) refresh_errors: BTreeMap<AccountId, String>,
    /// Why the newest settings write was refused or failed, when it was.
    /// `None` is the state of every write that landed.
    pub(crate) error: Option<SettingsError>,
}

impl<P: Provider> App<P> {
    /// The settings snapshot, for the host's pull after a
    /// [`Surface::Settings`](crate::Surface::Settings) signal.
    #[must_use]
    pub fn settings(&self) -> SettingsSnapshot {
        self.settings.get()
    }

    /// Installs the host's account store, the seam the settings surface
    /// reads the account configs through and writes them back over. Call it
    /// before the first settings dispatch: an app with no seam installed
    /// renders its account rows without host info and refuses the dav and
    /// removal verbs.
    pub fn install_account_ops(&self, ops: Arc<dyn HostAccountOps<P>>) {
        *self.account_ops.lock().expect("account-ops mutex poisoned") = Some(ops);
    }

    /// The installed account store, cloned out from behind its mutex.
    fn account_ops(&self) -> Option<Arc<dyn HostAccountOps<P>>> {
        self.account_ops
            .lock()
            .expect("account-ops mutex poisoned")
            .clone()
    }

    /// The accounts whose dav editor last refused, for the removal's test.
    #[cfg(test)]
    pub(crate) fn dav_errors_for_test(&self) -> Vec<AccountId> {
        self.settings_state
            .lock()
            .expect("settings mutex poisoned")
            .dav_errors
            .keys()
            .cloned()
            .collect()
    }

    /// Routes one [`SettingsIntent`] to its handler. Every arm publishes the
    /// settings surface on completion, because every arm moves something the
    /// snapshot shows (a setting, an account's row, the newest error) or
    /// nothing at all when it refused, which is itself something the surface
    /// must hear.
    pub(crate) async fn dispatch_settings(&self, intent: SettingsIntent) {
        match intent {
            SettingsIntent::Refresh => self.rebuild_settings().await,
            SettingsIntent::SetTheme { mode } => {
                self.set_setting(|prefs| prefs.theme = mode).await;
            }
            SettingsIntent::SetClock { twenty_four } => {
                self.set_setting(|prefs| prefs.twenty_four_clock = twenty_four)
                    .await;
            }
            SettingsIntent::SetZone { zone } => self.set_zone(zone).await,
            SettingsIntent::SetFirstDayOfWeek { weekday } => {
                self.set_setting(|prefs| prefs.first_day_of_week = weekday)
                    .await;
            }
            SettingsIntent::SetHorizon { days } => self.set_horizon(days).await,
            SettingsIntent::SetQuoteStyle { below } => {
                self.set_setting(|prefs| prefs.quote_below = below).await;
            }
            SettingsIntent::SetViewMode { mode } => {
                self.set_view_mode(mode).await;
            }
            SettingsIntent::SetDefaultCalendar { calendar } => {
                self.set_setting(|prefs| prefs.default_calendar = calendar)
                    .await;
            }
            SettingsIntent::SetDav { account, section } => {
                self.set_dav(account, section).await;
            }
            SettingsIntent::RemoveAccount { account } => {
                self.remove_account(account).await;
            }
        }
    }

    /// The shared tail of a total set arm: write the field, persist it, and
    /// answer the persist's failure on the rebuilt snapshot. A refusal
    /// earlier in an arm never reaches here.
    async fn set_setting(&self, write: impl FnOnce(&mut Preferences)) {
        let save_failed = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .set_setting(write);
        self.answer(save_failed.then_some(SettingsError::PersistFailed))
            .await;
    }

    /// The zone arm: the id must resolve through the engine's zone resolver,
    /// because every floating and all-day instant in the calendar resolves
    /// through the display zone and a zone the engine cannot localise is a
    /// diary that cannot be laid out. A refusal keeps the standing zone.
    async fn set_zone(&self, zone: String) {
        let resolvable =
            engine_api::TimeZoneId::iana(&zone).is_ok_and(|id| engine_api::is_supported_zone(&id));
        if !resolvable {
            self.answer(Some(SettingsError::ZoneUnresolvable)).await;
            return;
        }
        self.set_setting(|prefs| prefs.zone = zone).await;
        // The calendar resolves through the zone, so it republishes beside
        // the settings block: the same change is a change to both surfaces.
        // The re-expansion comes first, because a floating event's stored
        // instant is only correct for the zone it was expanded under; the
        // accounts whose re-expansion refused keep old-zone instants, so
        // their rows say a retry is owed while the zone itself stands.
        let stranded = self.reexpand_calendar().await;
        self.answer_zone_reexpansion(&stranded).await;
        self.rebuild_calendar().await;
    }

    /// Records the zone change's per-account re-expansion outcomes and
    /// republishes: the accounts whose re-expansion refused carry the
    /// sentence, the rest clear, so the channel states the newest zone
    /// change the way the others do. The zone itself stands, because it
    /// persisted; this names the accounts whose stored occurrences still
    /// resolve through the old one.
    async fn answer_zone_reexpansion(&self, stranded: &[AccountId]) {
        {
            let mut state = self.settings_state.lock().expect("settings mutex poisoned");
            state.refresh_errors.clear();
            for account in stranded {
                state.refresh_errors.insert(
                    account.clone(),
                    "the diary could not be re-expanded under the new zone; set the zone again to retry"
                        .to_owned(),
                );
            }
        }
        self.rebuild_settings().await;
    }

    /// The horizon arm: the cold window may span a week to a year, the
    /// bounds the rebuild's own clamp enforces on read.
    async fn set_horizon(&self, days: u32) {
        if !(crate::preferences::HORIZON_MIN_DAYS..=crate::preferences::HORIZON_MAX_DAYS)
            .contains(&days)
        {
            self.answer(Some(SettingsError::HorizonOutOfBounds)).await;
            return;
        }
        self.set_setting(|prefs| prefs.horizon_days = days).await;
    }

    /// The view-mode path both intents share: the grouping is a persisted
    /// setting and the mailbox list's own state, so one write rides the
    /// shared settings tail (persist, then answer the persist's failure on
    /// the snapshot), resets the list's window, rebuilds the list, and
    /// answers on the settings surface beside it. The list intent's own arm
    /// delegates here.
    pub(crate) async fn set_view_mode(&self, mode: ViewMode) {
        let save_failed = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .set_setting(|prefs| prefs.view_mode = mode);
        self.reset_window();
        self.bump_generation();
        self.rebuild_snapshot().await;
        self.answer(save_failed.then_some(SettingsError::PersistFailed))
            .await;
    }

    /// Records the newest outcome on the state and republishes the settings
    /// surface.
    async fn answer(&self, error: Option<SettingsError>) {
        self.settings_state
            .lock()
            .expect("settings mutex poisoned")
            .error = error;
        self.rebuild_settings().await;
    }

    /// Rebuilds the settings snapshot and publishes it on
    /// [`Surface::Settings`](crate::Surface::Settings): the persisted
    /// settings block, one row per configured account, and the about block.
    pub(crate) async fn rebuild_settings(&self) {
        let settings = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .settings_block();
        let infos = match self.account_ops() {
            Some(ops) => ops.account_infos().await,
            None => Vec::new(),
        };
        let dav_errors = self
            .settings_state
            .lock()
            .expect("settings mutex poisoned")
            .dav_errors
            .clone();
        let refresh_errors = self
            .settings_state
            .lock()
            .expect("settings mutex poisoned")
            .refresh_errors
            .clone();
        let accounts = {
            let handles = self.account_handles().await;
            let mut accounts = Vec::with_capacity(handles.len());
            for handle in &handles {
                let info = infos.iter().find(|info| info.id == handle.id);
                accounts.push(astra_viewmodel::AccountSettingsRow {
                    legs: astra_viewmodel::AccountLegs {
                        calendar: handle
                            .providers
                            .iter()
                            .any(|provider| provider.connection_info().capabilities.calendars()),
                        contacts: !self.people_legs(&handle.id).is_empty(),
                    },
                    account: handle.id.clone(),
                    address: handle.identity.email.clone(),
                    kind: info.map_or(astra_viewmodel::AccountKind::ImapSmtp, |info| info.kind),
                    dav: info.and_then(|info| info.dav.clone()),
                    dav_error: dav_errors.get(&handle.id).cloned(),
                    refresh_error: refresh_errors.get(&handle.id).cloned(),
                    grant: info.and_then(|info| info.grant),
                });
            }
            // The one exception to the join rule: an account whose grant the
            // host reports dead never dialled, so no handle stands for it,
            // and the row it renders anyway is the only surface that can
            // offer the re-grant. Its address comes from the info the host
            // answered; its legs stand on nothing.
            for info in infos {
                if handles.iter().any(|handle| handle.id == info.id) {
                    continue;
                }
                if let Some(grant) = info.grant {
                    accounts.push(astra_viewmodel::AccountSettingsRow {
                        legs: astra_viewmodel::AccountLegs {
                            calendar: false,
                            contacts: false,
                        },
                        account: info.id.clone(),
                        address: info.address.clone(),
                        kind: info.kind,
                        dav: info.dav.clone(),
                        dav_error: dav_errors.get(&info.id).cloned(),
                        refresh_error: refresh_errors.get(&info.id).cloned(),
                        grant: Some(grant),
                    });
                }
            }
            accounts
        };
        let error = self
            .settings_state
            .lock()
            .expect("settings mutex poisoned")
            .error;
        let save_failed = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .last_save_failed();
        self.settings.publish(SettingsSnapshot {
            settings,
            accounts,
            about: about_block(),
            error: error.or(save_failed.then_some(SettingsError::PersistFailed)),
        });
    }
}

/// The about block: the version and the attributions, the core's answer
/// rather than the web's guess. The version is the workspace package
/// version, which the `VERSION` file moves with: one source by rule.
fn about_block() -> AboutBlock {
    AboutBlock {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        attributions: vec![
            "email-calendar-sync-engine (the mail and calendar engine)".to_owned(),
            "the pi SDK (the agent sidecar)".to_owned(),
            "React, TypeScript, and Vite (the web interface)".to_owned(),
        ],
    }
}

#[cfg(test)]
#[path = "../settings_set_tests.rs"]
mod set_tests;

#[cfg(test)]
#[path = "../settings_calendar_tests.rs"]
mod calendar_tests;

#[cfg(test)]
#[path = "../settings_dav_tests.rs"]
mod dav_tests;

#[cfg(test)]
#[path = "../settings_remove_tests.rs"]
mod remove_tests;

#[cfg(test)]
#[path = "../settings_accounts_tests.rs"]
mod accounts_tests;
