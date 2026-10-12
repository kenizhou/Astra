// SPDX-License-Identifier: GPL-3.0-only
//! The settings view-model: an immutable snapshot a host pulls after a
//! `settings` signal and renders as the settings screen, the accounts page,
//! and About.
//!
//! Pure projection like the people and calendar snapshots beside it: the
//! persisted preferences and the host's account configs are joined here
//! once, and every surface shares one definition of what is on screen. A
//! row's dav echo carries no password: the snapshot answers whether one is
//! stored, never the credential itself.

use engine_api::{AccountId, CalendarId};
use serde::{Deserialize, Serialize};

use crate::ViewMode;

/// An immutable settings snapshot for a surface to render: the persisted
/// settings block, one row per configured account, and the about block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsSnapshot {
    /// The persisted settings, every field of which the screen renders.
    pub settings: SettingsBlock,
    /// One row per configured account, for the accounts page. Empty crosses
    /// no key, and a surface renders the page's empty state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<AccountSettingsRow>,
    /// The version and the attributions, the core's answer rather than the
    /// web's guess.
    pub about: AboutBlock,
}

/// The persisted settings the screen renders: the preferences document's
/// settings fields projected as one block, every setting a surface can
/// change named beside the value it holds now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsBlock {
    /// The theme: follow the host, or an explicit light or dark override.
    pub theme: ThemeMode,
    /// Whether the clock is 24-hour; 12-hour otherwise.
    pub twenty_four_clock: bool,
    /// The IANA id the calendar lays its days out in.
    pub zone: String,
    /// The day the week starts on.
    pub first_day_of_week: WeekdayWire,
    /// How many days the calendar's cold window covers.
    pub horizon_days: u32,
    /// Whether the composer quotes the original below the reply.
    pub quote_below: bool,
    /// The mailbox list's grouping.
    pub view_mode: ViewMode,
    /// The calendar new events land in unless the user picks another;
    /// `None` when none is pinned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_calendar: Option<DefaultCalendar>,
}

/// The theme the chrome renders: follow the host, or an explicit override.
///
/// Defined in the view-model, beside the settings block that carries it, so
/// the intent that changes the theme, the snapshot that carries it, and the
/// preferences that persist it name one type; re-exported by the protocol
/// to keep it the one import a host needs. The web's own enum mirrors these
/// words, gaining `follow-host` beside them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Follow the host's theme: light or dark as the desktop itself is.
    #[default]
    FollowHost,
    /// Light, whatever the host is.
    Light,
    /// Dark, whatever the host is.
    Dark,
}

/// A day of the week, crossing as the engine's own weekday words (`mo`
/// through `su`).
///
/// A mirror of the engine's weekday type rather than the type itself,
/// because the preferences and the snapshot default to Monday (the engine's
/// own default for the week's start) and the engine's type carries no
/// `Default`. The words are pinned equal to the engine's in the tests, so a
/// first day stated here and a repeat rule's `WKST` spell one day the same
/// way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WeekdayWire {
    /// Monday.
    #[default]
    Mo,
    /// Tuesday.
    Tu,
    /// Wednesday.
    We,
    /// Thursday.
    Th,
    /// Friday.
    Fr,
    /// Saturday.
    Sa,
    /// Sunday.
    Su,
}

/// The calendar new events land in unless the user picks another: the
/// account and the calendar key together, because a calendar key is unique
/// only within its account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultCalendar {
    /// The account whose provider serves the calendar.
    pub account: AccountId,
    /// The calendar's key, unique within `account`.
    pub calendar: CalendarId,
}

/// One configured account, as the accounts page renders it: its address,
/// its kind, what it is used for, and its dav section when it carries one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSettingsRow {
    /// The account the row names.
    pub account: AccountId,
    /// The account's own address.
    pub address: String,
    /// The account's kind. Crosses as a snake_case word.
    pub kind: AccountKind,
    /// Which of the account's legs are standing.
    pub legs: AccountLegs,
    /// The dav section's echo, when the account carries one. The password
    /// never crosses back:
    /// [`dav_password_set`](DavSectionRow::dav_password_set) answers whether
    /// one is stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dav: Option<DavSectionRow>,
}

/// An account's kind, as the row states it. Crosses as a snake_case word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// IMAP mail with SMTP submission.
    ImapSmtp,
    /// JMAP.
    Jmap,
    /// DAV, with no mail half.
    Dav,
}

/// Which of an account's legs are standing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountLegs {
    /// Whether the calendar leg is standing.
    pub calendar: bool,
    /// Whether the contacts leg is standing.
    pub contacts: bool,
}

/// The dav section an account's row echoes back: the editor's shape minus
/// the password.
///
/// The credential never crosses back over the IPC; `dav_password_set`
/// answers whether one is stored, so the editor can say a password is saved
/// without ever holding it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DavSectionRow {
    /// The CalDAV origin, when the section names one. Absent, the account
    /// dials no calendar leg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caldav_url: Option<String>,
    /// The CardDAV origin, when the section names one of its own. Absent,
    /// the contacts leg falls back to the CalDAV origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carddav_url: Option<String>,
    /// The login both legs present.
    pub username: String,
    /// Whether a password is stored for the section.
    pub dav_password_set: bool,
}

/// The About block: the version and the attributions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AboutBlock {
    /// The version, from the one source of truth.
    pub version: String,
    /// The names the about screen credits. Empty crosses no key.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributions: Vec<String>,
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
