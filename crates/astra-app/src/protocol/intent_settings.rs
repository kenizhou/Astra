// SPDX-License-Identifier: GPL-3.0-only
//! The settings half of the inbound protocol ([`SettingsIntent`]), which
//! [`Intent::Settings`](super::Intent::Settings) carries.
//!
//! Split from [`super::intent`] for the 500-line limit, following
//! [`PeopleIntent`](super::PeopleIntent). One family for one surface: every
//! set verb persists its value into the preferences document before the
//! snapshot republishes, so a changed setting is on disk by the time the
//! screen shows it and survives a restart by construction.
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

use engine_api::AccountId;
use serde::{Deserialize, Serialize};

use crate::reference::CalendarRef;

use super::ViewMode;

pub use astra_viewmodel::{ThemeMode, WeekdayWire};

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
    /// Set the mailbox list's grouping. The settings family's own arm; the
    /// list intent beside it keeps its wire and delegates.
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
    /// Remove an account: its stored config, its rows, and its legs go
    /// together.
    RemoveAccount {
        /// The account to remove.
        account: AccountId,
    },
}
