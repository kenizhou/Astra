// SPDX-License-Identifier: GPL-3.0-only
//! Persisted app preferences: the mailbox-list view mode, which sidebar trees are shut,
//! the selected folder, and the settings block (theme, clock, zone, first day of the
//! week, horizon, quote style, default calendar). Host app preferences, not synced data,
//! so they live in a small TOML file beside the engine store rather than in the store
//! itself.
//!
//! The expansion map is stored **inverted** relative to the question every caller asks:
//! the file holds what is collapsed, so an account or folder nobody has touched (and
//! every one of them on a first launch) opens showing what is inside it. The accessors
//! keep that inversion in one place, so no caller reads the set directly and gets the
//! answer backwards.
//!
//! [`PreferencesState`] is the runtime's in-memory copy plus the path it writes through:
//! a toggle changes the copy, then persists with a read-modify-write against what is on
//! disk right now, so a concurrent write to an unrelated preference is not clobbered.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use astra_viewmodel::{FolderRow, ThemeMode, WeekdayWire};
use serde::{Deserialize, Serialize};

use crate::{
    reference::{CalendarRef, FolderRef},
    signatures::AccountSignatures,
    ViewMode,
};

/// The preferences file's name, in the app data directory beside the engine store.
const FILE_NAME: &str = "preferences.toml";

/// App-level preferences persisted across launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    /// The persisted mailbox-list grouping (flat vs threaded). Defaults to
    /// [`ViewMode::Flat`] when the field is absent (an older preferences file, or first
    /// boot).
    #[serde(default)]
    pub view_mode: ViewMode,
    /// The accounts whose folder tree is **shut** in the sidebar, by account id.
    ///
    /// The collapsed ones, not the expanded ones, so an account nobody has touched
    /// opens showing its folders. Read through [`Preferences::account_expanded`], never
    /// off the field: the inversion is exactly the kind a direct reader gets backwards.
    /// A [`BTreeSet`] so the serialised TOML order is stable across writes.
    #[serde(default)]
    pub collapsed_accounts: BTreeSet<String>,
    /// The folders whose own sub-folders are **shut**, by account id then folder key.
    ///
    /// Keyed on both halves because a folder key is unique only within its account:
    /// every provider calls its inbox `inbox`, and the pane holds every account's tree
    /// at once. Nested rather than one set of joined strings because a folder key is a
    /// provider's own string and may hold whatever separator a join picked. Read it
    /// through [`Preferences::folder_expanded`].
    #[serde(default)]
    pub collapsed_folders: BTreeMap<String, BTreeSet<String>>,
    /// The folder that was showing when the app last stopped, so a relaunch reopens it;
    /// `None` outside a folder view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_folder: Option<FolderRef>,
    /// The account whose own view was showing when the app last stopped, so a
    /// relaunch reopens it; `None` beside a folder selection or the unified
    /// "all inboxes" view. An older preferences file holds no entry, which is
    /// the unified default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_account: Option<engine_api::AccountId>,
    /// The persisted theme: follow the host, or an explicit light or dark
    /// override.
    #[serde(default)]
    pub theme: ThemeMode,
    /// Whether the clock is 24-hour; 12-hour (the default) otherwise.
    #[serde(default)]
    pub twenty_four_clock: bool,
    /// The IANA id the calendar lays its days out in.
    #[serde(default = "default_zone")]
    pub zone: String,
    /// The day the week starts on, in the engine's own weekday words.
    #[serde(default)]
    pub first_day_of_week: WeekdayWire,
    /// How many days the calendar's cold window covers.
    #[serde(default = "default_horizon")]
    pub horizon_days: u32,
    /// Whether the composer quotes the original below the reply.
    #[serde(default = "default_true")]
    pub quote_below: bool,
    /// The calendar new events land in unless the user picks another;
    /// `None` when none is pinned, which writes no entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_calendar: Option<CalendarRef>,
    /// The account plain compositions open from: the new-message button and
    /// the `mailto:` entry. `None` when unset, which the first standing
    /// account answers; a request (reply, forward) always keeps its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_send_account: Option<engine_api::AccountId>,
    /// Each account's signature assignment, by account id. An account
    /// absent from the map seeds no signature, and its last empty slot
    /// takes its row with it, so the file holds only live assignments.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub signatures: BTreeMap<engine_api::AccountId, AccountSignatures>,
}

/// The shipped defaults, which the serde field defaults and a first launch
/// name as one set. Manual rather than derived because the zone, the
/// horizon, and the quote style are not the std defaults of their types.
impl Default for Preferences {
    fn default() -> Self {
        Self {
            view_mode: ViewMode::default(),
            collapsed_accounts: BTreeSet::new(),
            collapsed_folders: BTreeMap::new(),
            selected_folder: None,
            selected_account: None,
            theme: ThemeMode::default(),
            twenty_four_clock: false,
            zone: default_zone(),
            first_day_of_week: WeekdayWire::default(),
            horizon_days: default_horizon(),
            quote_below: default_true(),
            default_calendar: None,
            default_send_account: None,
            signatures: BTreeMap::new(),
        }
    }
}

/// The calendar's display zone until the user picks one.
fn default_zone() -> String {
    "Etc/UTC".to_owned()
}

/// The calendar's cold window, in days.
fn default_horizon() -> u32 {
    42
}

/// The shortest horizon a setting may name: a window shorter than a week
/// cannot hold the week a cold grid paints.
pub(crate) const HORIZON_MIN_DAYS: u32 = 7;

/// The longest horizon a setting may name: a year's span is the most a cold
/// window may be asked to materialise in one go.
pub(crate) const HORIZON_MAX_DAYS: u32 = 365;

/// The shipped composer default: the quoted original below the reply.
fn default_true() -> bool {
    true
}

impl Preferences {
    /// Whether `account`'s folder tree is open in the sidebar.
    ///
    /// **Expanded is the default**, which is why the persisted set holds the collapsed
    /// accounts rather than the expanded ones: an account nobody has touched, including
    /// one just added, opens showing its folders, the way a mail client that has just
    /// been given a mailbox should.
    #[must_use]
    pub fn account_expanded(&self, account: &str) -> bool {
        !self.collapsed_accounts.contains(account)
    }

    /// Records whether `account`'s folder tree is open. Expanding drops the entry
    /// rather than writing one, so the file holds only the accounts the user actually
    /// shut.
    pub fn set_account_expanded(&mut self, account: &str, expanded: bool) {
        if expanded {
            self.collapsed_accounts.remove(account);
        } else {
            self.collapsed_accounts.insert(account.to_owned());
        }
    }

    /// Whether `folder`'s own sub-folders are showing, in `account`'s tree. Expanded is
    /// the default, as it is for an account: a folder nobody has touched shows what is
    /// inside it, so nothing goes missing across the launch this ships in.
    #[must_use]
    pub fn folder_expanded(&self, account: &str, folder: &str) -> bool {
        !self
            .collapsed_folders
            .get(account)
            .is_some_and(|shut| shut.contains(folder))
    }

    /// Records whether `folder`'s sub-folders are showing. Expanding drops the entry,
    /// and the account's last entry takes the account's own map row with it, so the
    /// file holds only what the user actually shut rather than growing a row per folder
    /// they ever opened.
    pub fn set_folder_expanded(&mut self, account: &str, folder: &str, expanded: bool) {
        if expanded {
            let Some(shut) = self.collapsed_folders.get_mut(account) else {
                return;
            };
            shut.remove(folder);
            if shut.is_empty() {
                self.collapsed_folders.remove(account);
            }
        } else {
            self.collapsed_folders
                .entry(account.to_owned())
                .or_default()
                .insert(folder.to_owned());
        }
    }

    /// Records the folder the list is showing, or `None` outside a folder view.
    pub fn set_selected_folder(&mut self, folder: Option<FolderRef>) {
        self.selected_folder = folder;
    }

    /// The account whose own view was showing, or `None` beside a folder
    /// selection or the unified view.
    #[must_use]
    pub fn selected_account(&self) -> Option<engine_api::AccountId> {
        self.selected_account.clone()
    }

    /// Records the account whose own view the list is showing, or `None`
    /// beside a folder selection or the unified view.
    pub fn set_selected_account(&mut self, account: Option<engine_api::AccountId>) {
        self.selected_account = account;
    }

    /// Drops every expansion this account holds, its selected folder when the
    /// selection pointed inside it, and its own selected view: used when the
    /// account is removed, so a later re-add opens showing its folders rather
    /// than inheriting a shut tree nobody remembers shutting. Returns whether
    /// anything was stored for it.
    pub fn remove_account(&mut self, account: &str) -> bool {
        let tree = self.collapsed_accounts.remove(account);
        let folders = self.collapsed_folders.remove(account).is_some();
        let selected = self
            .selected_folder
            .take_if(|folder| folder.account.as_str() == account)
            .is_some();
        let viewed = self
            .selected_account
            .take_if(|held| held.as_str() == account)
            .is_some();
        let held = self.signatures.len();
        self.signatures.retain(|id, _| id.as_str() != account);
        let signatures = self.signatures.len() < held;
        let send_default = self
            .default_send_account
            .take_if(|held| held.as_str() == account)
            .is_some();
        tree || folders || selected || viewed || signatures || send_default
    }
}

/// The preferences file's path inside the app data directory `base`.
///
/// Derived here so every reader and writer names the same file the app writes; boot
/// hands the engine store's directory, and the preferences sit beside it.
#[must_use]
pub fn preferences_path(base: impl AsRef<Path>) -> PathBuf {
    base.as_ref().join(FILE_NAME)
}

/// Loads preferences from `path`. A missing, unreadable, or unparseable file yields
/// defaults rather than an error: a preferences file is best-effort state, and a host
/// that cannot read it simply starts from the defaults on next boot.
#[must_use]
pub fn load_preferences(path: impl AsRef<Path>) -> Preferences {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|body| toml::from_str(&body).ok())
        .unwrap_or_default()
}

/// Writes `prefs` to `path` as TOML, creating parent directories as needed.
///
/// # Errors
///
/// Returns a [`std::io::Error`] if the parent directory or file cannot be written (a
/// TOML serialisation failure is mapped to [`std::io::ErrorKind::InvalidData`], though
/// a flat preferences struct never triggers it in practice).
pub fn save_preferences(path: impl AsRef<Path>, prefs: &Preferences) -> std::io::Result<()> {
    let body = toml::to_string(prefs)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body)
}

/// The runtime's loaded preferences, and the path they write through.
///
/// Held in memory and consulted on every snapshot rebuild, so it cannot be a file read;
/// the persisted copy is written only when the user actually changes something.
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
    pub(crate) fn view_mode(&self) -> ViewMode {
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
    pub(crate) fn selected_folder(&self) -> Option<FolderRef> {
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
        folder: Option<FolderRef>,
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
    /// map, and the selected folder when the selection pointed inside it)
    /// and persists. Returns whether anything was stored for it.
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
            .clamp(HORIZON_MIN_DAYS, HORIZON_MAX_DAYS) as u16
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
            default_calendar: prefs.default_calendar.as_ref().map(|pin| {
                astra_viewmodel::DefaultCalendar {
                    account: pin.account.clone(),
                    key: pin.key.clone(),
                }
            }),
        }
    }

    /// Stamps the persisted expansion over one account's sorted rows, which
    /// [`sorted_folder_rows`] leaves open everywhere: the group header from the account
    /// map, each folder's own chevron from the folder map. A folder that holds no
    /// folders is never expanded; there is no tree to open, so a client drawing a
    /// chevron from this cannot draw one that does nothing.
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
        on_disk.signatures.clone_from(&self.prefs.signatures);
        self.last_save_failed = save_preferences(path, &on_disk).is_err();
    }
}

#[cfg(test)]
#[path = "preferences_tests.rs"]
mod tests;
