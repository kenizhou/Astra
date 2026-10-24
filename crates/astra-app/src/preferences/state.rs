// SPDX-License-Identifier: GPL-3.0-only
//! The runtime's loaded preferences, and the path they write through. Split
//! from the document half of [`super`] so neither file grows past the cap.
//!
//! Held in memory and consulted on every snapshot rebuild, so it cannot be a file read;
//! the persisted copy is written only when the user actually changes something.

use std::path::PathBuf;

use astra_viewmodel::{FolderRow, SyncBodyCap, WeekdayWire};

use super::{
    load_preferences, save_preferences, Preferences, DEFAULT_BODY_CAP_OCTETS, DEFAULT_POLL_MINUTES,
    POLL_MAX_MINUTES, POLL_MIN_MINUTES,
};
use crate::accounts::NEW_ACCOUNT_SYNC_DEPTH_MONTHS;

pub(crate) struct PreferencesState {
    pub(crate) prefs: Preferences,
    path: Option<PathBuf>,
    last_save_failed: bool,
}

impl PreferencesState {
    /// Loads the persisted state (every tree open when the file is absent or
    /// unreadable); unpersisted when `path` is `None` (the tests, an in-memory build).
    pub(crate) fn new(path: Option<PathBuf>) -> Self {
        let prefs = path
            .as_ref()
            .map_or_else(Preferences::default, load_preferences);
        Self {
            prefs,
            path,
            last_save_failed: false,
        }
    }

    /// The persisted mailbox-list grouping.
    pub(crate) fn view_mode(&self) -> crate::ViewMode {
        self.prefs.view_mode
    }

    /// Whether `account`'s folder tree is open; `true` for an account nobody has shut.
    pub(crate) fn account_expanded(&self, account: &str) -> bool {
        self.prefs.account_expanded(account)
    }

    /// Records whether `account`'s tree is open and persists it. Returns whether
    /// anything changed, so a client re-asserting the state it already has costs no
    /// disk write and no snapshot rebuild.
    pub(crate) fn set_account_expanded(&mut self, account: &str, expanded: bool) -> bool {
        if self.prefs.account_expanded(account) == expanded {
            return false;
        }
        self.prefs.set_account_expanded(account, expanded);
        self.persist();
        true
    }

    /// Records whether one folder's own sub-folders are showing, and persists.
    /// Returns whether anything changed, for the reason
    /// [`set_account_expanded`](Self::set_account_expanded) does.
    pub(crate) fn set_folder_expanded(
        &mut self,
        account: &str,
        folder: &str,
        expanded: bool,
    ) -> bool {
        if self.prefs.folder_expanded(account, folder) == expanded {
            return false;
        }
        self.prefs.set_folder_expanded(account, folder, expanded);
        self.persist();
        true
    }

    /// The persisted selected folder, or `None` outside a folder view.
    pub(crate) fn selected_folder(&self) -> Option<crate::reference::FolderRef> {
        self.prefs.selected_folder.clone()
    }

    /// The persisted account whose own view was showing, or `None` beside a
    /// folder selection or the unified view.
    pub(crate) fn selected_account(&self) -> Option<engine_api::AccountId> {
        self.prefs.selected_account()
    }

    /// Records the selected scope, folder, account, or unified view, as one
    /// write and persists it: the pair moves together, so neither half of the
    /// choice can land on disk without the other.
    pub(crate) fn set_selected_scope(
        &mut self,
        folder: Option<crate::reference::FolderRef>,
        account: Option<engine_api::AccountId>,
    ) {
        self.prefs.set_selected_folder(folder);
        self.prefs.set_selected_account(account);
        self.persist();
    }

    /// Writes one settings field and persists the document: the shared tail
    /// of every total settings arm. Returns whether the persist failed, so
    /// the arm can answer the failure on the snapshot; the in-memory copy
    /// moved either way, which is the honest state of a host that is running
    /// the new setting while the disk holds the old one.
    pub(crate) fn set_setting(&mut self, write: impl FnOnce(&mut Preferences)) -> bool {
        write(&mut self.prefs);
        self.persist();
        self.last_save_failed
    }

    /// Drops every account-scoped entry the account holds (its expansion
    /// map, the selected folder when the selection pointed inside it, and
    /// its per-account rows) and persists. Returns whether anything was
    /// stored for it.
    pub(crate) fn remove_account(&mut self, account: &str) -> bool {
        let removed = self.prefs.remove_account(account);
        if removed {
            self.persist();
        }
        removed
    }

    /// Whether the newest persist failed. A write that lands clears it, so
    /// the flag states the newest write the way the snapshot's error channel
    /// does.
    pub(crate) fn last_save_failed(&self) -> bool {
        self.last_save_failed
    }

    /// The IANA id the calendar lays its days out in, from the persisted
    /// preference.
    pub(crate) fn zone(&self) -> String {
        self.prefs.zone.clone()
    }

    /// The day the week starts on, from the persisted preference.
    pub(crate) fn first_day_of_week(&self) -> WeekdayWire {
        self.prefs.first_day_of_week
    }

    /// The cold window's length in days, clamped into the bounds the horizon
    /// setting enforces so a hand-edited file cannot ask the rebuild for a
    /// window the product would itself refuse.
    pub(crate) fn horizon_days(&self) -> u16 {
        self.prefs
            .horizon_days
            .clamp(super::HORIZON_MIN_DAYS, super::HORIZON_MAX_DAYS) as u16
    }

    /// One account's own sync settings as the app runs them: the stored row
    /// with every unset field answered by the product default, the one
    /// projection the pass window, the body cap, the search depth note, and
    /// the settings row all read, so the four can never disagree.
    pub(crate) fn sync_settings(
        &self,
        account: &engine_api::AccountId,
    ) -> astra_viewmodel::AccountSyncSettings {
        let Some(row) = self.prefs.sync.get(account) else {
            return Self::default_sync_settings();
        };
        astra_viewmodel::AccountSyncSettings {
            depth_months: row.depth_months.unwrap_or(NEW_ACCOUNT_SYNC_DEPTH_MONTHS),
            body_cap: row.body_cap.unwrap_or(SyncBodyCap::Bytes {
                octets: DEFAULT_BODY_CAP_OCTETS,
            }),
            poll_minutes: row
                .poll_minutes
                .unwrap_or(DEFAULT_POLL_MINUTES)
                .clamp(POLL_MIN_MINUTES, POLL_MAX_MINUTES),
        }
    }

    /// The settings every account runs with when it holds no stored row.
    fn default_sync_settings() -> astra_viewmodel::AccountSyncSettings {
        astra_viewmodel::AccountSyncSettings {
            depth_months: NEW_ACCOUNT_SYNC_DEPTH_MONTHS,
            body_cap: SyncBodyCap::Bytes {
                octets: DEFAULT_BODY_CAP_OCTETS,
            },
            poll_minutes: DEFAULT_POLL_MINUTES,
        }
    }

    /// The persisted settings block, projected for the settings snapshot:
    /// every field the screen renders, named beside the value it holds now.
    pub(crate) fn settings_block(&self) -> astra_viewmodel::SettingsBlock {
        let prefs = &self.prefs;
        astra_viewmodel::SettingsBlock {
            theme: prefs.theme,
            twenty_four_clock: prefs.twenty_four_clock,
            zone: prefs.zone.clone(),
            first_day_of_week: prefs.first_day_of_week,
            horizon_days: prefs.horizon_days,
            quote_below: prefs.quote_below,
            view_mode: prefs.view_mode,
            plugins_enabled: prefs.plugins_enabled,
            notifications_enabled: prefs.notifications_enabled,
            debug_enabled: prefs.debug_enabled,
            analytics_consented: prefs.analytics_consented,
            language: prefs.language.clone(),
            zone_offered: prefs.zone_offered,
            default_calendar: prefs.default_calendar.as_ref().map(|pin| {
                astra_viewmodel::DefaultCalendar {
                    account: pin.account.clone(),
                    key: pin.key.clone(),
                }
            }),
            default_send_account: prefs.default_send_account.clone(),
        }
    }

    /// Stamps the persisted expansion over one account's sorted rows, which
    /// [`sorted_folder_rows`] leaves open everywhere: the group header from the account
    /// map, each folder's own chevron from the folder map. A folder that holds no
    /// folders is never expanded; there is no tree to open, so a client drawing a
    /// chevron from this cannot draw one that does nothing.
    ///
    /// [`sorted_folder_rows`]: crate::snapshot::sorted_folder_rows
    pub(crate) fn stamp(&self, account: &str, rows: &mut [FolderRow]) {
        for row in rows {
            row.expanded =
                row.has_children && self.prefs.folder_expanded(account, row.key.as_str());
        }
    }

    /// Writes the state back, read-modify-write against what is on disk right now, so
    /// a concurrent write to an unrelated preference is not clobbered by our in-memory
    /// copy. A no-op without a path. Records whether a write was attempted and
    /// failed, the fact the snapshot's error channel answers from.
    fn persist(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        let mut on_disk = load_preferences(path);
        on_disk.view_mode = self.prefs.view_mode;
        on_disk
            .collapsed_accounts
            .clone_from(&self.prefs.collapsed_accounts);
        on_disk
            .collapsed_folders
            .clone_from(&self.prefs.collapsed_folders);
        on_disk
            .selected_folder
            .clone_from(&self.prefs.selected_folder);
        on_disk
            .selected_account
            .clone_from(&self.prefs.selected_account);
        on_disk.theme = self.prefs.theme;
        on_disk.twenty_four_clock = self.prefs.twenty_four_clock;
        on_disk.zone.clone_from(&self.prefs.zone);
        on_disk.first_day_of_week = self.prefs.first_day_of_week;
        on_disk.horizon_days = self.prefs.horizon_days;
        on_disk.quote_below = self.prefs.quote_below;
        on_disk
            .default_calendar
            .clone_from(&self.prefs.default_calendar);
        on_disk
            .default_send_account
            .clone_from(&self.prefs.default_send_account);
        on_disk.plugins_enabled = self.prefs.plugins_enabled;
        on_disk.notifications_enabled = self.prefs.notifications_enabled;
        on_disk.debug_enabled = self.prefs.debug_enabled;
        on_disk.analytics_consented = self.prefs.analytics_consented;
        on_disk.language.clone_from(&self.prefs.language);
        on_disk.zone_offered = self.prefs.zone_offered;
        on_disk.zone_picked = self.prefs.zone_picked;
        on_disk.signatures.clone_from(&self.prefs.signatures);
        on_disk.sync.clone_from(&self.prefs.sync);
        self.last_save_failed = save_preferences(path, &on_disk).is_err();
    }
}
