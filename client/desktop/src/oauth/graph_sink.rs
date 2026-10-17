// SPDX-License-Identifier: GPL-3.0-only
//! The desktop's [`TokenSink`]: the host half every OAuth token source
//! reports a rotation or a moved scope set to, whichever family the
//! account's grant belongs to (`[graph]` or `[google]`). One sink serves
//! every account, keyed by the id it is handed; the boot's restore and
//! the sign-in's completion build it over the same two stores (the dial
//! table and the credential transaction), so both paths land their
//! writes in the same places.

use std::sync::Arc;

use astra_oauth::GrantedScopes;
use engine_api::AccountId;

use astra_account::{AccountConfig, Secret, TokenSink};

use crate::dial_table::AccountDials;

use super::transaction::CredentialTransaction;

/// The entry's OAuth section, whichever family the account dials: every
/// family carries the same rotation slot the sink advances, so one sink
/// serves a Google or JMAP account exactly as it serves a Microsoft
/// one. A jmap grant keeps no granted-set slot (its scopes are the
/// grant's own, re-sent on refresh), so its pair answers none.
fn oauth_grant_mut(config: &mut AccountConfig) -> Option<(&mut Secret, &mut Option<Vec<String>>)> {
    if let Some(graph) = config.graph.as_mut() {
        Some((&mut graph.refresh_token, &mut graph.granted_scopes))
    } else if let Some(google) = config.google.as_mut() {
        Some((&mut google.refresh_token, &mut google.granted_scopes))
    } else {
        None
    }
}

/// The entry's jmap grant's refresh slot, when the account carries one:
/// the rotation a ratcheting JMAP server issues lands here, and a
/// dropped one kills the account at the next launch.
fn jmap_refresh_mut(config: &mut AccountConfig) -> Option<&mut Secret> {
    Some(&mut config.jmap.as_mut()?.oauth.as_mut()?.refresh_token)
}

/// The sink over the dial table and the credential transaction: the boot's
/// restore and the sign-in's completion both build one.
#[must_use]
pub(crate) fn graph_token_sink(
    dials: Arc<AccountDials>,
    credentials: Arc<dyn crate::credential_store::AccountCredentialStore>,
) -> Arc<dyn TokenSink> {
    Arc::new(DialTableTokenSink {
        dials,
        transaction: CredentialTransaction::new(credentials),
    })
}

/// The desktop's [`TokenSink`] over the dial table and the credential
/// transaction.
struct DialTableTokenSink {
    /// The registered accounts: the entry a rotation advances.
    dials: Arc<AccountDials>,
    /// The transaction the advanced document persists through.
    transaction: CredentialTransaction,
}

#[async_trait::async_trait]
impl TokenSink for DialTableTokenSink {
    async fn refresh_token_rotated(&self, account: &AccountId, new_refresh_token: &str) {
        let handle = astra_account::account_log_handle(account.as_str());
        let Some((mut config, _)) = self.dials.of(account.as_str()) else {
            // No entry at all: the rotation is lost. Every path that dials
            // an account goes through the dial table, so this is a fault
            // in the app rather than a schedule, which is why it shouts.
            log::error!(
                "oauth: [{handle}] the server renewed this account's sign-in, but no registered \
                 account could save it, so the renewal was lost; mail keeps working until the \
                 app is restarted, after which this account asks to be signed in again"
            );
            return;
        };
        // Whichever family's grant the entry carries, microsoft, google,
        // or jmap: the rotation advances it, and a jmap grant has no
        // granted-set slot to move beside it.
        let refresh = if let Some((refresh, _)) = oauth_grant_mut(&mut config) {
            refresh
        } else if let Some(refresh) = jmap_refresh_mut(&mut config) {
            refresh
        } else {
            // Registered, but with no grant to advance: a stored-secret
            // account handed a rotation it cannot use is silent.
            return;
        };
        *refresh = Secret::new(new_refresh_token.to_owned());
        let document = match config.to_toml() {
            Ok(document) => document,
            Err(err) => {
                log::error!(
                    "oauth: [{handle}] the renewed sign-in could not be prepared for storage \
                     ({err}); mail keeps working until the app is restarted, after which this \
                     account asks to be signed in again"
                );
                self.dials.replace_config(account.as_str(), config);
                return;
            }
        };
        self.dials.replace_config(account.as_str(), config);
        // The rotation path owns the refusal wording: nothing rolls back,
        // the token this replaced is already spent.
        if self.transaction.rotate(account, &document).is_ok() {
            log::info!(
                "oauth: [{handle}] the server renewed this account's sign-in; the new one is \
                 saved to this device's secure store"
            );
        }
    }

    /// Stores what the provider now grants, when it moved; the entry keeps
    /// the set so the refusal surfaces can read it offline. Losing this
    /// write costs less than losing a rotation: the next refresh names the
    /// scopes again, and nothing is spent.
    async fn scopes_granted(&self, account: &AccountId, granted: &GrantedScopes) {
        let handle = astra_account::account_log_handle(account.as_str());
        let Some((mut config, _)) = self.dials.of(account.as_str()) else {
            return;
        };
        let Some((_, granted_slot)) = oauth_grant_mut(&mut config) else {
            return;
        };
        // The response is what the provider grants now; consent withdrawn
        // since the last one shows up nowhere else.
        if granted_slot
            .as_ref()
            .is_some_and(|stored| granted.same_as(stored))
        {
            return;
        }
        *granted_slot = Some(granted.as_slice().to_vec());
        let Ok(document) = config.to_toml() else {
            log::warn!(
                "oauth: [{handle}] the provider now grants this account a different permission \
                 set, but it could not be prepared for storage; the next renewal tries again"
            );
            self.dials.replace_config(account.as_str(), config);
            return;
        };
        self.dials.replace_config(account.as_str(), config);
        // A scopes-only write spends nothing: a refused persist keeps the
        // last rotation's document, and the next renewal tries again, so
        // the refusal is a warning here rather than the rotation error
        // the transaction's own log would name.
        if self.transaction.rotate(account, &document).is_ok() {
            log::info!(
                "oauth: [{handle}] the provider now grants this account {} permission(s); saved \
                 to this device's secure store",
                granted.as_slice().len(),
            );
        }
    }
}
