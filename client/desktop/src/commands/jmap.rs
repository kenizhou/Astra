// SPDX-License-Identifier: GPL-3.0-only
//! The wizard's JMAP add command: the password leg of the JMAP form.
//! The URL derivation (a blank server derives the email's domain, a
//! bare host gains `https://`, an explicit scheme survives) runs
//! host-side in `astra-account`'s one builder, so the form sends the
//! fields it collected and the document it gets back is the same one
//! every stored JMAP document walks; the add then rides the shared
//! transaction (`dial before persist`, a refused store rolls the whole
//! add back), the SP2h dav pattern the IMAP add already runs.

use tauri::State;

use engine_api::AccountId;

use crate::{AppState, HostProvider};

use super::{add_account_with, AddedAccount, CommandError};

/// Builds the `[jmap]` document from the form's fields and adds the
/// account through the shared transaction: derive, dial, persist,
/// register, join, first sync. `dial` is injected so the flow is
/// testable without a server; production blocks on the account layer's
/// own connect through the runtime's driver thread.
///
/// # Errors
///
/// Answers [`CommandError::Config`] when the fields cannot build a
/// document (a blank email or secret), and the shared transaction's own
/// errors beyond it; a refused dial or store leaves nothing registered.
pub(crate) fn add_jmap_account_with<D>(
    state: &AppState,
    email: &str,
    server_url: Option<&str>,
    password: &str,
    dial: D,
) -> Result<AddedAccount, CommandError>
where
    D: FnOnce(
        &astra_account::AccountConfig,
        &AccountId,
    ) -> Result<
        (
            Vec<HostProvider>,
            std::sync::Arc<astra_account::ImapConnections>,
        ),
        astra_account::AccountError,
    >,
{
    let setup = astra_account::JmapSetup {
        email: email.to_owned(),
        server_url: server_url.map(str::to_owned),
        password: password.to_owned(),
    };
    let config_toml = astra_account::build_jmap_config_toml(&setup)
        .map_err(|err| CommandError::Config(err.to_string()))?;
    add_account_with(state, &config_toml, dial)
}

/// Adds one JMAP account from the form's fields: the email, the server
/// URL (blank derives the email's domain), and the one secret. Dials
/// the account-wide provider, persists, registers, and spawns the first
/// sync.
///
/// # Errors
///
/// Answers [`CommandError`] when the fields cannot build a document,
/// the dial fails, or the store refuses the write; a refusal leaves
/// nothing registered.
// Tauri's command macro hands `State` by value; the wrapper reads and dials through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) async fn add_jmap_account(
    state: State<'_, AppState>,
    email: String,
    server_url: Option<String>,
    password: String,
) -> Result<AddedAccount, CommandError> {
    let dial = |config: &astra_account::AccountConfig, _id: &AccountId| {
        let connections = astra_account::ImapConnections::new();
        let providers = state.runtime.block_on(|| async {
            let tls =
                astra_account::account_tls(config).map_err(astra_account::AccountError::Tls)?;
            let provider = astra_account::dial_jmap_account(config, &tls).await?;
            Ok(vec![Box::new(provider) as HostProvider])
        });
        providers.map(|providers| (providers, connections))
    };
    add_jmap_account_with(&state, &email, server_url.as_deref(), &password, dial)
}

#[cfg(test)]
#[path = "jmap_tests.rs"]
mod tests;
