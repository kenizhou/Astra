// SPDX-License-Identifier: GPL-3.0-only
//! The Google half of an account: the token source its providers dial
//! through, the refreshing Gmail and Google Calendar providers over the
//! engine's adapters, and the mapping from what an account is used for
//! to what its sign-in asks for.
//!
//! The token source is the shared
//! [`GraphTokenSource`](crate::GraphTokenSource) in its Google costume:
//! the type is provider-neutral (an OAuth client, a refresh token, and
//! the rotation sink), so a Google account inherits the single-flight,
//! the expiry skew, and the refusal classification exactly as a
//! Microsoft one does. Google's deltas live in the stored `[google]`
//! document rather than the source: the Desktop client's
//! non-confidential secret rides the token endpoint on the exchange AND
//! the refresh, so it persists in the document for a launch-later
//! refresh to send again, and Google does not rotate its refresh tokens
//! in practice, so the rotation write stays as a backstop that rarely
//! fires but is not optional.
//!
//! Gmail's mail sync is **account-global**: the history cursor is
//! account-wide, so one provider covers every label and message (the
//! JMAP shape, not Graph's per-folder fan-out). The calendar binds one
//! provider to the account's primary calendar.

mod address;
mod calendar;
mod dial;
mod provider;
mod token_source;

pub use address::{fetch_google_address, fetch_google_address_at};
pub use calendar::connect_google_calendar_providers;
pub use dial::{dial_google_account, GoogleProviders};
use provider::RefreshingGmailProvider;
pub use token_source::google_token_source;

use std::sync::Arc;

use astra_oauth::scopes::{ProviderScopes, ScopeGroup};
use engine_api::Provider;
use engine_tls::TlsClientConfig;
use time::Date;

use crate::{AccountError, Capability, GraphTokenSource};

#[cfg(test)]
pub(crate) mod test_support;

/// The scope group a capability asks for at `provider`.
const fn group(provider: &ProviderScopes, capability: Capability) -> &ScopeGroup {
    match capability {
        Capability::Mail => &provider.mail,
        Capability::Calendar => &provider.calendar,
        Capability::Contacts => &provider.contacts,
        Capability::Colleagues => &provider.colleagues,
    }
}

/// The uses whose scopes are asked for: colleagues only beside contacts,
/// because the directory is only ever read as part of the address book.
fn asked(capabilities: &[Capability]) -> impl Iterator<Item = Capability> + '_ {
    capabilities.iter().copied().filter(|capability| {
        *capability != Capability::Colleagues || capabilities.contains(&Capability::Contacts)
    })
}

/// The scopes a sign-in for an account used for `capabilities` requests
/// at `provider`: the provider's `always` set plus each chosen use's
/// group.
///
/// What ships reaches a sign-in as mail and calendar; the contacts and
/// directory groups ride their slices, and the mapping covers them for
/// the day they do.
#[must_use]
pub fn requested_scopes(provider: &ProviderScopes, capabilities: &[Capability]) -> Vec<String> {
    provider
        .always
        .iter()
        .chain(asked(capabilities).flat_map(|capability| group(provider, capability).requested))
        .map(|scope| (*scope).to_owned())
        .collect()
}

/// The uses in `capabilities` that `granted` does not allow: chosen,
/// asked for, and refused.
///
/// `granted` is `None` for a grant stored before the granted set was
/// recorded, which withholds nothing until a refresh says otherwise. A
/// use is withheld only when its **needed** scope is missing; a missing
/// scope that serves one feature within it is refused when that feature
/// is reached.
#[must_use]
pub fn withheld(
    provider: &ProviderScopes,
    capabilities: &[Capability],
    granted: Option<&[String]>,
) -> Vec<Capability> {
    let Some(granted) = granted else {
        return Vec::new();
    };
    asked(capabilities)
        .filter(|capability| !provider.holds(granted, group(provider, *capability).needed))
        .collect()
}

/// Connects the Gmail mail provider a Google account syncs: **one**
/// account-global provider (Gmail's message scope is account-wide, the
/// JMAP shape, unlike Graph's per-folder fan-out). Probes the credential
/// up front with one refresh, so a revoked refresh token surfaces as a
/// connect failure now, not on the first background sync.
///
/// # Errors
///
/// Returns [`AccountError::SigninRejected`] when the stored grant is
/// dead (the re-auth signal), or [`AccountError::Graph`] on a transient
/// refresh failure.
pub async fn connect_google_mail_providers(
    tokens: Arc<GraphTokenSource>,
    since: Option<Date>,
    tls: &TlsClientConfig,
) -> Result<Vec<Box<dyn Provider>>, AccountError> {
    // Cheap credential probe: one refresh, so a dead refresh token fails
    // here rather than at the first background sync.
    let _ = tokens.access_token().await?;
    Ok(vec![Box::new(RefreshingGmailProvider::new(
        tokens,
        since,
        tls.clone(),
    ))])
}

/// Builds an on-demand Gmail provider for a Google account, sharing its
/// `tokens`. Gmail's provider is account-wide (one scope covers every
/// label), so the returned provider serves any folder the host opens;
/// there is no per-folder binding. The token is minted lazily on the
/// first call.
pub fn connect_google_folder(
    tokens: Arc<GraphTokenSource>,
    since: Option<Date>,
    tls: &TlsClientConfig,
) -> Box<dyn Provider> {
    Box::new(RefreshingGmailProvider::new(tokens, since, tls.clone()))
}

#[cfg(test)]
#[path = "scopes_tests.rs"]
mod scopes_tests;
