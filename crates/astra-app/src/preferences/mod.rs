// SPDX-License-Identifier: GPL-3.0-only
//! Persisted app preferences: the list's view mode and expansion, the last
//! selection, the settings block, the per-account sync settings, the
//! signature assignments, and the plugin policy switch. Host app
//! preferences, not synced data, so they live in a small TOML file beside
//! the engine store rather than in the store itself.
//!
//! The expansion map is stored **inverted**: the file holds what is
//! collapsed, so an untouched account or folder opens showing what is inside
//! it; read through the accessors, never the set directly.
//!
//! The runtime's in-memory copy and the path it writes through live in
//! [`state`], split from this document half so neither grows past the file
//! cap; a toggle there changes the copy, then persists with a
//! read-modify-write against what is on disk right now, so a concurrent
//! write to an unrelated preference is not clobbered.

mod state;

pub(crate) use state::PreferencesState;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use astra_viewmodel::{SyncBodyCap, ThemeMode, WeekdayWire};
use serde::{Deserialize, Serialize};

use crate::{
    reference::{CalendarRef, FolderRef},
    signatures::AccountSignatures,
    ViewMode,
};

/// The preferences file's name, in the app data directory beside the engine store.
const FILE_NAME: &str = "preferences.toml";

/// The per-message body cap an account warms and keeps when the user has set
/// none. A message larger than this is pathological mail, not a setting the
/// person made, so the default warms effectively everything a mailbox holds
/// while still bounding the one number that could grow without end.
pub(crate) const DEFAULT_BODY_CAP_OCTETS: u64 = 256 * 1024 * 1024;

/// The shortest cadence the background loop may poll at; a faster manual
/// refresh is the refresh button.
pub(crate) const POLL_MIN_MINUTES: u16 = 15;

/// The longest cadence the poll may wait between passes.
pub(crate) const POLL_MAX_MINUTES: u16 = 120;

/// The cadence an account polls at when the user has set none.
pub const DEFAULT_POLL_MINUTES: u16 = 30;

/// App-level preferences persisted across launches.
///
/// The independent toggles are deliberately plain bools, each answering its
/// own question on the settings screen.
#[allow(clippy::struct_excessive_bools)]
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
    /// The folder that was showing when the app last stopped, so a relaunch
    /// reopens it; `None` outside a folder view.
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
    /// Each account's signature assignment, by account id; the file holds
    /// only live assignments.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub signatures: BTreeMap<engine_api::AccountId, AccountSignatures>,
    /// Whether plugins run at all (the admin policy switch); on by default.
    #[serde(default = "default_true")]
    pub plugins_enabled: bool,
    /// Whether new mail raises OS notifications; on by default. The
    /// notification switch the settings screen carries and the host's
    /// delivery loop reads, so a person who wants no announcements gets
    /// none whatever the window state.
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    /// Each account's own sync settings, by account id; the file holds only
    /// the fields the user set, and an absent row or field falls back to the
    /// product defaults.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sync: BTreeMap<engine_api::AccountId, AccountSyncSettings>,
}

/// The shipped defaults, which the serde field defaults and a first launch
/// name as one set.
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
            plugins_enabled: true,
            notifications_enabled: true,
            sync: BTreeMap::new(),
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

/// The composer default and the policy switch share the on-by-default answer.
fn default_true() -> bool {
    true
}

/// One account's own sync settings as the document persists them: every
/// field optional, so an edit of one leaves the others standing and an
/// absent row or field is the product default rather than a stored zero.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSyncSettings {
    /// How many months of history the account syncs; zero is the whole
    /// mailbox.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth_months: Option<u16>,
    /// The body cap state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_cap: Option<SyncBodyCap>,
    /// The polling cadence, in minutes; clamped into its bounds on write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_minutes: Option<u16>,
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

    /// Merges one account's sync settings: each named field writes (the
    /// cadence clamped into its bounds), each absent field keeps what stood,
    /// and a field written back to the product default stores as unset, so
    /// the default and unset are one state and a row left with nothing set
    /// comes out of the file, the same discipline the expansion maps follow.
    pub fn set_sync_settings(
        &mut self,
        account: &engine_api::AccountId,
        depth_months: Option<u16>,
        body_cap: Option<SyncBodyCap>,
        poll_minutes: Option<u16>,
    ) {
        let clamped_poll =
            poll_minutes.map(|minutes| minutes.clamp(POLL_MIN_MINUTES, POLL_MAX_MINUTES));
        let mut row = self
            .sync
            .get(account)
            .cloned()
            .unwrap_or(AccountSyncSettings::default());
        if let Some(depth_months) = depth_months {
            row.depth_months = (depth_months != crate::accounts::NEW_ACCOUNT_SYNC_DEPTH_MONTHS)
                .then_some(depth_months);
        }
        if let Some(body_cap) = body_cap {
            row.body_cap = match body_cap {
                SyncBodyCap::Bytes { octets } if octets == DEFAULT_BODY_CAP_OCTETS => None,
                other => Some(other),
            };
        }
        if let Some(poll_minutes) = clamped_poll {
            row.poll_minutes = (poll_minutes != DEFAULT_POLL_MINUTES).then_some(poll_minutes);
        }
        if row == AccountSyncSettings::default() {
            self.sync.remove(account);
        } else {
            self.sync.insert(account.clone(), row);
        }
    }

    /// Drops every expansion this account holds, its selected folder when the
    /// selection pointed inside it, its own selected view, and its own rows
    /// in the per-account maps (the signatures, the sync settings): used when
    /// the account is removed, so a later re-add opens showing its folders
    /// rather than inheriting a shut tree nobody remembers shutting. Returns
    /// whether anything was stored for it.
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
        let sync = engine_api::AccountId::try_from(account)
            .ok()
            .and_then(|id| self.sync.remove(&id))
            .is_some();
        tree || folders || selected || viewed || signatures || send_default || sync
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

#[cfg(test)]
#[path = "../preferences_tests.rs"]
mod tests;
