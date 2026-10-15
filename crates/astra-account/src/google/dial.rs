// SPDX-License-Identifier: GPL-3.0-only
//! The Google account dial: the one function every path that brings a
//! Google account up calls (the boot, the sign-in's completion), over
//! the shared token source.
//!
//! Gmail's mail half is **one account-global provider** (the history
//! cursor is account-wide, the JMAP shape), and the calendar half binds
//! one refreshing provider to the account's primary calendar, appended
//! after the mail provider the way every account's calendar leg rides.
//! Both legs spend the same token, so one refresh serves the whole dial.
//!
//! The calendar leg is never a re-auth case: Google asks for the
//! calendar scope at sign-in, so a refused calendar is an error the
//! dial carries, never a banner. The mail leg is the account's: its
//! failure fails the dial, and a dead grant arrives on it as
//! [`AccountError::SigninRejected`].

use std::sync::Arc;

use engine_api::{AccountId, Provider};

use crate::{account_tls, AccountConfig, AccountError, GraphTokenSource};

use super::calendar::connect_google_calendar_providers;
use super::connect_google_mail_providers;

/// The providers a Google account dials, and the thing about its
/// calendar leg a caller may need to act on.
pub struct GoogleProviders {
    /// The account's mail provider: one account-global Gmail provider,
    /// because Gmail's message scope covers every label.
    pub mail: Vec<Box<dyn Provider>>,
    /// The primary calendar's provider, when the calendar connected;
    /// appended after the mail provider, the shape the
    /// capability-scoped calendar selection expects.
    pub calendar: Option<Box<dyn Provider>>,
    /// The account-labelled error from a calendar connect that failed;
    /// the account's mail stands and the agenda is empty. Carried rather
    /// than logged away so a caller can surface it; an empty calendar is
    /// otherwise indistinguishable from a server that has none.
    pub calendar_error: Option<String>,
}

impl core::fmt::Debug for GoogleProviders {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The providers hold live connections; the dialled legs are the
        // whole safe description.
        f.debug_struct("GoogleProviders")
            .field("mail", &self.mail.len())
            .field("calendar", &self.calendar.is_some())
            .field("calendar_error", &self.calendar_error)
            .finish()
    }
}

/// Dials a Google account's providers: its account-global Gmail provider
/// and the provider for its primary calendar, over the one shared token
/// source. The one function every path that brings a Google account up
/// calls, so boot, sign-in, and reconnect cannot disagree about what a
/// Google account opens.
///
/// `account` is the account's stored document: both legs run under its
/// shared TLS policy, so the certificates its owner accepted pin into
/// the Google connections exactly as they do the mail half's. The
/// credential half of the document arrived earlier, as `tokens`.
///
/// # Errors
///
/// Returns [`AccountError`] when the mail leg fails: the initial token
/// refresh (a dead grant arrives as [`AccountError::SigninRejected`]) or
/// the credential probe. A calendar that fails costs the dial nothing:
/// the error is carried in the outcome for the caller to surface.
pub async fn dial_google_account(
    account_id: &AccountId,
    tokens: Arc<GraphTokenSource>,
    account: &AccountConfig,
) -> Result<GoogleProviders, AccountError> {
    let tls = account_tls(account)?;
    // The same Google token serves both legs, concurrently: the mail
    // probe and the calendar list share the source's single refresh
    // flight, so a dial mints at most one access token however many
    // legs it opens.
    let (mail, calendar) = tokio::join!(
        connect_google_mail_providers(Arc::clone(&tokens), None, &tls),
        connect_google_calendar_providers(account_id, Arc::clone(&tokens), &tls),
    );
    let mail = mail?;
    // The calendar leg is never a re-auth case: Google asks for the
    // scope at sign-in, so every refusal is an error the dial carries.
    let (calendar, calendar_error) = match calendar {
        Ok(provider) => (Some(provider), None),
        Err(err) => {
            log::warn!("google: the calendar leg failed to dial; mail stands: {err}");
            (None, Some(err.to_string()))
        }
    };
    Ok(GoogleProviders {
        mail,
        calendar,
        calendar_error,
    })
}

#[cfg(test)]
#[path = "dial_tests.rs"]
mod tests;
