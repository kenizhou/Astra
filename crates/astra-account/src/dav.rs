// SPDX-License-Identifier: GPL-3.0-only
//! The DAV half of an account: the optional `[dav]` section of a stored
//! config that names the calendar and contacts origins, and the dial that
//! connects both providers.
//!
//! Both legs dial under the account's shared TLS policy, so the certificate
//! exceptions its owner accepted pin into them exactly as the mail half's
//! connections; a plain-HTTP origin (the harness's) verifies nothing and
//! needs no exception. Discovery is the engine's: the adapters resolve the
//! RFC 6764 well-known paths and the homes behind them inside `connect`.

use engine_api::{ContactsProvider, Provider};
use provider_caldav::{CalDavConfig, CalDavProvider, CardDavConfig, CardDavProvider, Credentials};
use serde::Deserialize;

use crate::{account_tls, AccountConfig, AccountError, ConfigError, Secret};

/// The optional `[dav]` section of a stored account: the origins both legs
/// dial and the credential they present.
///
/// The CardDAV leg falls back to the CalDAV origin when the section gives it
/// none of its own (nearly every server that speaks one DAV dialect speaks
/// both at the same origin); a section naming neither origin refuses at load.
#[derive(Debug, Clone, Deserialize)]
pub struct DavAccount {
    /// The CalDAV origin (e.g. `https://dav.example.net`), when the section
    /// names one. Absent, the account dials no calendar leg.
    #[serde(default)]
    pub caldav_url: Option<String>,
    /// The CardDAV origin, when the section names one apart from the CalDAV
    /// origin. Absent, the contacts leg falls back to the CalDAV origin.
    #[serde(default)]
    pub carddav_url: Option<String>,
    /// The login both legs present.
    pub username: String,
    /// The login password (or app-specific password).
    #[serde(default)]
    pub password: Option<Secret>,
}

impl DavAccount {
    /// The CalDAV origin this section names, when it names one: blank text
    /// names no server.
    #[must_use]
    pub fn caldav_origin(&self) -> Option<&str> {
        non_blank(self.caldav_url.as_deref())
    }

    /// The CardDAV origin this section dials, after the fallback: its own
    /// when the section carries one, the CalDAV origin otherwise.
    #[must_use]
    pub fn carddav_origin(&self) -> Option<&str> {
        non_blank(self.carddav_url.as_deref()).or_else(|| self.caldav_origin())
    }

    /// Refuses a section that names no server at all: there is nothing to
    /// dial, and a stored account that silently lost its origins is a bug to
    /// surface at load, not a calendar that quietly never syncs.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::NoDavOrigin`] when neither origin stands.
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        if self.caldav_origin().is_none() && self.carddav_origin().is_none() {
            return Err(ConfigError::NoDavOrigin);
        }
        Ok(())
    }

    /// The engine credentials both legs present, or `None` when the section
    /// stores no password.
    #[must_use]
    pub fn credentials(&self) -> Option<Credentials> {
        self.password.as_ref().map(|password| Credentials::Basic {
            username: self.username.clone(),
            password: password.expose().to_owned(),
        })
    }
}

/// The providers a dav section dials: the calendar leg to append to the
/// account's provider list, and the contacts leg the host installs beside
/// it. Either half stands alone when the section names only its origin.
pub struct DavProviders {
    /// The connected CalDAV provider, when the section names a calendar
    /// origin.
    pub caldav: Option<Box<dyn Provider>>,
    /// The connected CardDAV provider, when an origin stands after the
    /// fallback.
    pub carddav: Option<Box<dyn ContactsProvider>>,
}

impl core::fmt::Debug for DavProviders {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The providers hold live connections; the dialled legs are the whole
        // safe description.
        f.debug_struct("DavProviders")
            .field("caldav", &self.caldav.is_some())
            .field("carddav", &self.carddav.is_some())
            .finish()
    }
}

/// Dials the account's dav legs: the CalDAV provider when the section names
/// a calendar origin, the CardDAV provider when a contacts origin stands
/// after the fallback, both under the account's shared TLS policy. An
/// account with no dav section dials nothing and succeeds empty.
///
/// # Errors
///
/// Returns [`AccountError::MissingCredential`] when the section stores no
/// password, and [`AccountError::Dav`] when either dial fails: the caller
/// keeps the account's mail half standing and logs.
pub async fn connect_dav_providers(config: &AccountConfig) -> Result<DavProviders, AccountError> {
    let Some(dav) = config.dav.as_ref() else {
        return Ok(DavProviders {
            caldav: None,
            carddav: None,
        });
    };
    let tls = account_tls(config)?;
    let credentials = dav
        .credentials()
        .ok_or(AccountError::MissingCredential("dav"))?;
    let mut providers = DavProviders {
        caldav: None,
        carddav: None,
    };
    if let Some(origin) = dav.caldav_origin() {
        let caldav = CalDavProvider::connect(
            CalDavConfig::new(origin.to_owned(), credentials.clone()).with_tls(tls.clone()),
        )
        .await
        .map_err(|err| AccountError::Dav(err.to_string()))?;
        providers.caldav = Some(Box::new(caldav));
    }
    if let Some(origin) = dav.carddav_origin() {
        let carddav = CardDavProvider::connect(
            CardDavConfig::new(origin.to_owned(), credentials).with_tls(tls),
        )
        .await
        .map_err(|err| AccountError::Dav(err.to_string()))?;
        providers.carddav = Some(Box::new(carddav));
    }
    Ok(providers)
}

/// `Some` for text that names something, `None` for blank text.
fn non_blank(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
#[path = "dav_tests.rs"]
mod tests;
