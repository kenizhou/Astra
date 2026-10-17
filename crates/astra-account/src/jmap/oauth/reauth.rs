// SPDX-License-Identifier: GPL-3.0-only
//! Signing an existing OAuth JMAP account back in, when the server has
//! stopped accepting its stored grant: the expired grant's own button.
//!
//! A dead grant is not a setup problem, so it does not go back through
//! the setup form. [`begin_jmap_reauth`] re-runs the **same**
//! authorisation against the account's persisted grant: its endpoints,
//! its registered client id, its redirect URI, its scopes, and its
//! RFC 8707 resource indicator, and nothing else. That is what those
//! fields are persisted for, and it is the only way to be sure the
//! re-authorisation asks the *same* server, as the *same* registered
//! client, for the *same* scopes. Re-running RFC 7591 registration
//! instead would mint a **second** client id on the person's account and
//! leave the first one orphaned.
//!
//! [`complete_jmap_reauth`] exchanges the redirect and hands back the
//! fresh section; the host swaps it in place through its credential
//! transaction, so the account keeps its identity, its settings, and its
//! downloaded mail.

use time::OffsetDateTime;

use super::{exchange, start_login, JmapLoginStart};
use crate::{jmap::JmapAccount, AccountError};

/// Starts a re-authentication of an existing OAuth JMAP account: builds a
/// fresh PKCE authorization URL from the section's **persisted** grant,
/// for the host to open exactly as it does a first sign-in. Pass the
/// redirect back to [`complete_jmap_reauth`].
///
/// Makes no network calls and re-registers nothing: the grant's own
/// endpoints, client id, redirect URI, scopes, and RFC 8707 resource are
/// the authorization's every parameter, and that is what they are
/// persisted for.
///
/// # Errors
///
/// Returns [`AccountError::Jmap`] when the section stores no grant: a
/// password account has no browser flow to re-run, and its remedy is a
/// fresh secret in Settings.
pub fn begin_jmap_reauth(section: &JmapAccount) -> Result<JmapLoginStart, AccountError> {
    let grant = section.oauth.as_ref().ok_or_else(|| {
        AccountError::Jmap(
            "this jmap account signs in with a stored password, which has no sign-in to re-run"
                .to_owned(),
        )
    })?;
    log::info!(
        "jmap oauth: re-authenticating an existing account against its stored authorization \
         endpoint {} (no re-discovery, no re-registration)",
        grant.authorize_endpoint,
    );
    start_login(section.email.clone(), section.base_url.clone(), grant)
}

/// Completes a re-authentication started by [`begin_jmap_reauth`]:
/// exchanges the redirect for a fresh grant and returns the section to
/// store in the account's place. The account keeps its identity; only the
/// credential changes, and the host re-persists the result through its
/// credential transaction.
///
/// # Errors
///
/// Returns [`AccountError::Jmap`] if `pending` is malformed or the
/// exchange failed, and, distinctively, when the sign-in was completed as
/// a **different mailbox** than the section's account: writing that grant
/// under the account's id would point one person's config at another's
/// mail while still displaying the original address, so it is refused and
/// the account's stored grant is left untouched.
pub async fn complete_jmap_reauth(
    stored: &JmapAccount,
    pending: &str,
    callback_url: &str,
    now: OffsetDateTime,
) -> Result<JmapAccount, AccountError> {
    let fresh = exchange(pending, callback_url, now).await?;
    same_mailbox(stored, &fresh)?;
    Ok(fresh)
}

/// Checks that the section just signed into is the one being
/// reconnected.
///
/// The authorization page lets the person pick; the `login_hint` targets
/// an address, it does not pin one: so someone with two mailboxes at the
/// same provider can complete the flow as the wrong one. Both sides are
/// compared as their sign-ins name them (the derived account id), never a
/// pinned id, which says which account this is rather than which mailbox
/// a browser just opened.
///
/// # Errors
///
/// Returns [`AccountError::Jmap`] naming the mismatch.
fn same_mailbox(stored: &JmapAccount, fresh: &JmapAccount) -> Result<(), AccountError> {
    let expected = stored
        .derived_account_id()
        .map_err(|err| AccountError::Jmap(err.to_string()))?;
    let signed_in = fresh
        .derived_account_id()
        .map_err(|err| AccountError::Jmap(err.to_string()))?;
    if signed_in != expected {
        log::warn!(
            "jmap oauth: the re-authentication signed in a DIFFERENT account than the one being \
             reconnected; discarding the new grant",
        );
        return Err(AccountError::Jmap(
            "that sign-in is for a different account than the one being reconnected".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../jmap_reauth_tests.rs"]
mod tests;
