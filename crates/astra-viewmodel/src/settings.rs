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

/// One account's signature assignment, as a settings row carries it: what
/// its new mail opens with and what its replies open with. Either slot may
/// stand empty while the other holds one, and both absent crosses no key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSignatures {
    /// What a plain new message from this account seeds; absent seeds nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_mail: Option<astra_composer::Signature>,
    /// What a reply or forward from this account seeds; absent seeds nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<astra_composer::Signature>,
}

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
    /// Why the newest settings write was refused or failed, when it was. The
    /// value on screen is the one that stands; this names why a write the
    /// user just made is not it. Absent crosses no key, the state of every
    /// write that landed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SettingsError>,
}

/// Why one settings write refused or failed, as the surface states it.
/// Crosses as a snake_case word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsError {
    /// The write landed in memory but the preferences file did not take it;
    /// a relaunch loses the change until a later write persists.
    PersistFailed,
    /// The zone id names no zone the engine can resolve, so the calendar
    /// could never lay a day out in it.
    ZoneUnresolvable,
    /// The horizon sits outside the bounds a cold window may span.
    HorizonOutOfBounds,
    /// The account's removal could not be completed; nothing went.
    RemoveFailed,
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
    /// The account plain compositions open from: the new-message button and
    /// the `mailto:` entry. `None` when unset, which the first standing
    /// account answers; a request (reply, forward) always keeps its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_send_account: Option<AccountId>,
    /// Whether plugins run at all (the admin policy switch). On by default.
    #[serde(default = "default_plugins_enabled")]
    pub plugins_enabled: bool,
}

/// The switch's default: plugins run unless the preference says otherwise.
fn default_plugins_enabled() -> bool {
    true
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
    /// The calendar's key, unique within `account`, spelled as the
    /// reference intent spells it.
    pub key: CalendarId,
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
    /// The account's crypto method (`none` / `openpgp` / `smime`): the
    /// built-in send gate's per-account switch, echoed from the stored
    /// config. `none` is the zero-cost default.
    #[serde(default)]
    pub crypto_method: String,
    /// Why the account's crypto-method write last refused or failed, when
    /// it did. Absent crosses no key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crypto_error: Option<String>,
    /// Why the account's dav editor last refused or failed, when it did: the
    /// section the editor shows is the one that stands, and this names why
    /// the save did not change it. Absent crosses no key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dav_error: Option<String>,
    /// Why the account's last zone change could not rewrite its stored
    /// occurrences, when it could not: the zone stands, but this account's
    /// diary still resolves through the one it was expanded under, and a
    /// later zone change re-expands and clears it. Absent crosses no key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_error: Option<String>,
    /// The standing grant refusal the row carries, when the account's
    /// OAuth grant needs the person to act: a withheld calendar or a dead
    /// sign-in. Absent is the healthy state and crosses no key, so a
    /// snapshot from a build before this field reads back healthy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant: Option<GrantState>,
    /// The account's signature assignment, when it holds one: what its new
    /// mail and its replies open with. Absent crosses no key, the state of
    /// an account that seeds nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signatures: Option<AccountSignatures>,
}

/// Why a Microsoft account's grant needs the person to act, as the
/// refusal surfaces state it. Crosses as a snake_case word. The word set
/// is the ported classification's: a grant too narrow (the calendar the
/// account was opened for went missing) and a grant gone (the sign-in
/// expired or was withdrawn), which are different prompts with the same
/// remedy, a fresh sign-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantState {
    /// The grant withholds the calendar: the mail stands and the agenda
    /// waits for a fresh sign-in to re-consent.
    CalendarWithheld,
    /// The grant is dead: the sign-in expired or was withdrawn, nothing
    /// about the account syncs, and the account never dialled. Never an
    /// outage word.
    SignInExpired,
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
    /// A Microsoft account over Graph, signed in through the browser.
    Microsoft,
    /// A Google account over its native APIs, signed in through the
    /// browser.
    Google,
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
