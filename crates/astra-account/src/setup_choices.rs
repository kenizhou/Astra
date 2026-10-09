// SPDX-License-Identifier: GPL-3.0-only
//! Which of mail, calendar and contacts a setup route offers, and how each starts: decided
//! once here, so every surface that renders the found card renders the same set.

use serde::{Deserialize, Serialize};

use crate::SetupRecommendation;

/// One thing an account can be used for.
///
/// `Colleagues` is a sub-choice of `Contacts` on a Microsoft or Google account: the people
/// in the person's work or school directory, which a provider grants as a separate
/// permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    /// Reading and sending mail.
    Mail,
    /// The account's calendar.
    Calendar,
    /// The account's own contacts.
    Contacts,
    /// The organisation's directory, beside the account's own contacts.
    Colleagues,
}

impl Capability {
    /// Every capability, in the order they are listed.
    pub const ALL: [Self; 4] = [Self::Mail, Self::Calendar, Self::Contacts, Self::Colleagues];
}

/// One use a setup route offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupChoice {
    /// The use.
    pub capability: Capability,
    /// Whether it starts switched on: its server was found, or the provider's sign-in covers it.
    pub on: bool,
    /// Whether its server is known. When it is not and the person switches the use on, the
    /// client asks for the server's URL.
    pub server_found: bool,
}

impl SetupRecommendation {
    /// The uses this route offers, in the order the client lists them. Empty for a route
    /// whose uses are not known before sign-in (JMAP, whose session says what it offers)
    /// and for the manual form.
    #[must_use]
    pub fn choices(&self) -> Vec<SetupChoice> {
        let found = |capability, server_found| SetupChoice {
            capability,
            on: server_found,
            server_found,
        };
        match self {
            Self::Imap {
                caldav_url,
                carddav_url,
                ..
            } => vec![
                found(Capability::Mail, true),
                found(Capability::Calendar, caldav_url.is_some()),
                // Contacts are looked for at the calendar's server when no address book was found.
                found(
                    Capability::Contacts,
                    caldav_url.is_some() || carddav_url.is_some(),
                ),
            ],
            Self::Dav {
                caldav_url,
                carddav_url,
                ..
            } => vec![
                found(Capability::Calendar, caldav_url.is_some()),
                found(
                    Capability::Contacts,
                    caldav_url.is_some() || carddav_url.is_some(),
                ),
            ],
            Self::Microsoft { .. } | Self::Google { .. } => Capability::ALL
                .into_iter()
                .map(|capability| found(capability, true))
                .collect(),
            Self::Jmap { .. } | Self::Manual { .. } => Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "setup_choices_tests.rs"]
mod tests;
