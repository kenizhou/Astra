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
    /// A [`BTreeSet`] so the serialized TOML order is stable across writes.
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
    /// `None` for the unified "all inboxes" view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_folder: Option<FolderRef>,
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
            theme: ThemeMode::default(),
            twenty_four_clock: false,
            zone: default_zone(),
            first_day_of_week: WeekdayWire::default(),
            horizon_days: default_horizon(),
            quote_below: default_true(),
            default_calendar: None,
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

    /// Records the folder the list is showing, or `None` for the unified view.
    pub fn set_selected_folder(&mut self, folder: Option<FolderRef>) {
        self.selected_folder = folder;
    }

    /// Drops every expansion this account holds, and its selected folder when the
    /// selection pointed inside it: used when the account is removed, so a later re-add
    /// opens showing its folders rather than inheriting a shut tree nobody remembers
    /// shutting. Returns whether anything was stored for it.
    pub fn remove_account(&mut self, account: &str) -> bool {
        let tree = self.collapsed_accounts.remove(account);
        let folders = self.collapsed_folders.remove(account).is_some();
        let selected = self
            .selected_folder
            .take_if(|folder| folder.account.as_str() == account)
            .is_some();
        tree || folders || selected
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
    prefs: Preferences,
    path: Option<PathBuf>,
}

impl PreferencesState {
    /// Loads the persisted state (every tree open when the file is absent or
    /// unreadable); unpersisted when `path` is `None` (the tests, an in-memory build).
    pub(crate) fn new(path: Option<PathBuf>) -> Self {
        let prefs = path
            .as_ref()
            .map_or_else(Preferences::default, load_preferences);
        Self { prefs, path }
    }

    /// The persisted mailbox-list grouping.
    pub(crate) fn view_mode(&self) -> ViewMode {
        self.prefs.view_mode
    }

    /// Records the grouping and persists it. Returns whether anything changed, so a
    /// host re-asserting the state it already has costs no disk write and no snapshot
    /// rebuild.
    pub(crate) fn set_view_mode(&mut self, mode: ViewMode) -> bool {
        if self.prefs.view_mode == mode {
            return false;
        }
        self.prefs.view_mode = mode;
        self.persist();
        true
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

    /// The persisted selected folder, or `None` for the unified view.
    pub(crate) fn selected_folder(&self) -> Option<FolderRef> {
        self.prefs.selected_folder.clone()
    }

    /// Records (or clears, with `None`) the selected folder and persists it.
    pub(crate) fn set_selected_folder(&mut self, folder: Option<FolderRef>) {
        self.prefs.set_selected_folder(folder);
        self.persist();
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
    /// copy. A no-op without a path.
    fn persist(&self) {
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
        on_disk.theme = self.prefs.theme;
        on_disk.twenty_four_clock = self.prefs.twenty_four_clock;
        on_disk.zone.clone_from(&self.prefs.zone);
        on_disk.first_day_of_week = self.prefs.first_day_of_week;
        on_disk.horizon_days = self.prefs.horizon_days;
        on_disk.quote_below = self.prefs.quote_below;
        on_disk
            .default_calendar
            .clone_from(&self.prefs.default_calendar);
        let _ = save_preferences(path, &on_disk);
    }
}

#[cfg(test)]
#[path = "preferences_tests.rs"]
mod tests;
