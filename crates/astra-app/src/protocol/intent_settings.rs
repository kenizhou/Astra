// SPDX-License-Identifier: GPL-3.0-only
//! The settings half of the inbound protocol ([`SettingsIntent`]), which
//! [`Intent::Settings`](super::Intent::Settings) carries.
//!
//! Split from [`super::intent`] for the 500-line limit, following
//! [`PeopleIntent`](super::PeopleIntent). One family for one surface: every
//! set verb answers by persisting its value into the preferences document
//! snapshot republishes by contract, so a changed setting is on disk by the
//! before the family answers (the dispatch arm enforces it).
//!
//! Two rules the family encodes:
//!
//! - **The zone is a string the core validates, not a wire type.** The
//!   browser owns the IANA list the picker offers; the intent carries the
//!   chosen id verbatim and the core refuses one it cannot resolve, so a
//!   host never needs a zone library of its own.
//! - **The dav password crosses in, never out.** [`SetDav`] carries the
//!   editor's section with its password; the settings snapshot's row echoes
//!   the section masked, stating only that one is stored.

use astra_composer::Signature;
use engine_api::AccountId;
use serde::{Deserialize, Serialize};

use crate::reference::CalendarRef;

use super::ViewMode;

pub use astra_viewmodel::{SyncBodyCap, ThemeMode, WeekdayWire};

/// The dav editor's shape: what the accounts page's form holds for one
/// account's dav section.
///
/// Every field optional, because the editor states only what the user
/// typed: an edit of the username alone crosses no key for the origins, the
/// same omission discipline a contact patch follows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DavSectionInput {
    /// The CalDAV origin, when the form holds one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caldav_url: Option<String>,
    /// The CardDAV origin, when the form holds one of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carddav_url: Option<String>,
    /// The login both legs present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// The login password, or app-specific password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

/// What the settings surface asks the runtime to do.
///
/// The verbs for the categories Astra has settings for; the absent
/// categories the spec pre-records stay absent, never stubbed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SettingsIntent {
    /// Rebuild and republish the settings snapshot.
    Refresh,
    /// Set the theme.
    SetTheme {
        /// The theme to apply.
        mode: ThemeMode,
    },
    /// Set the clock to 24-hour or 12-hour.
    SetClock {
        /// Whether the clock is 24-hour.
        twenty_four: bool,
    },
    /// Set whether plugins run at all (the admin policy switch).
    SetPluginsEnabled {
        /// On or off.
        enabled: bool,
    },
    /// Set the calendar's display zone.
    SetZone {
        /// The IANA id, carried verbatim; the core validates it on dispatch.
        zone: String,
    },
    /// Set the day the week starts on.
    SetFirstDayOfWeek {
        /// The first day, in the engine's own weekday words.
        weekday: WeekdayWire,
    },
    /// Set how many days the calendar's cold window covers.
    SetHorizon {
        /// The window length, in days.
        days: u32,
    },
    /// Set whether the composer quotes the original below the reply.
    SetQuoteStyle {
        /// Whether the quoted original sits below the reply.
        below: bool,
    },
    /// Set the mailbox list's grouping. The settings family's own arm (the
    /// list intent beside it keeps its own wire).
    SetViewMode {
        /// The grouping.
        mode: ViewMode,
    },
    /// Pin the calendar new events land in, or unpin it with `None`.
    SetDefaultCalendar {
        /// The calendar, or `None` to unpin.
        calendar: Option<CalendarRef>,
    },
    /// Write one account's dav section, or clear its legs with `None`.
    SetDav {
        /// The account whose section to write.
        account: AccountId,
        /// The editor's section, or `None` to clear the account's legs.
        section: Option<DavSectionInput>,
    },
    /// Write one account's crypto method (`none` / `openpgp` / `smime`):
    /// the built-in send gate's per-account switch. `none` is the zero-cost
    /// default (the engine's queue keeps every send).
    SetCryptoMethod {
        /// The account whose method to write.
        account: AccountId,
        /// The method word; the core refuses one it does not know.
        method: String,
    },
    /// Remove an account: its stored config, its rows, and its legs go
    /// together.
    RemoveAccount {
        /// The account to remove.
        account: AccountId,
    },
    /// Pin the account plain compositions open from, or unpin it with
    /// `None`; the first standing account answers an unpinned one.
    SetSendAccount {
        /// The default send account, or `None` to unpin.
        account: Option<AccountId>,
    },
    /// Write one slot of an account's signature assignment. `None` clears
    /// the slot, and the account's last empty slot takes its row with it.
    SetSignature {
        /// The account whose slot to write.
        account: AccountId,
        /// Whether the slot is the reply one; `false` is the new-mail one.
        reply: bool,
        /// The signature to seed from, or `None` to clear the slot.
        signature: Option<Signature>,
    },
    /// Write one account's own sync settings: the fetch depth, the body
    /// cap, and the polling cadence. Each field named is written and each
    /// absent field keeps what stood, the same omission discipline the dav
    /// editor's input follows; to return a field to the product default,
    /// set the default's value, since a default and an explicit equal are
    /// the same pass.
    SetSyncSettings {
        /// The account whose settings to write.
        account: AccountId,
        /// The fetch depth in months; zero is the whole mailbox.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        depth_months: Option<u16>,
        /// The body cap state.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body_cap: Option<SyncBodyCap>,
        /// The polling cadence in minutes, clamped into its 15 to 120
        /// bounds on write.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        poll_minutes: Option<u16>,
    },
}
