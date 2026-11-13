// SPDX-License-Identifier: GPL-3.0-only
//! The crypto receive command surface (SP4c-3c D2): the reading pane's
//! password re-entry. One command carries the reader's passphrase in-process
//! to the app core, which re-runs the password orchestrator over a fresh
//! source and republishes the reading snapshot. The answer is the prompt
//! affordance the snapshot now carries: `null` once the passphrase decrypts
//! (the words render, the dialog closes) or on a structural failure, and
//! `wrong_password` when it fit no slot and the dialog re-prompts.
//!
//! **Privacy.** The passphrase crosses the webview boundary into host memory
//! and moves straight into a [`SecretBox`] (zeroized on drop); it is never
//! logged, never persisted, and never recorded by the command's tests. A
//! cancel never reaches this command: the verdict stays needs-password.
//!
//! A child of [`super`], whose command registration this module continues,
//! under the same split the other command halves made for the line cap.

use astra_crypto_core::SecretBox;
use astra_viewmodel::PasswordPromptState;
use engine_api::{AccountId, ProviderKey};
use tauri::State;

use crate::AppState;

/// Re-opens the passphrase-protected message with `passphrase`: hands the
/// message identity and the secret to the app core on the runtime, answering
/// the prompt affordance the refreshed snapshot carries. The passphrase moves
/// into a [`SecretBox`] here and is never logged or persisted.
pub(crate) fn submit_reading_password_of(
    state: &AppState,
    account: &AccountId,
    key: &ProviderKey,
    passphrase: String,
) -> Option<PasswordPromptState> {
    let message = astra_app::MessageRef {
        account: account.clone(),
        key: key.clone(),
    };
    let secret = SecretBox::new(Box::new(passphrase));
    let app = std::sync::Arc::clone(&state.app);
    state
        .runtime
        .block_on(|| async move { app.submit_reading_password(&message, secret).await })
}

/// Submits the reader's passphrase for the open passphrase-protected message:
/// the reading pane's password dialog re-entry. The answer is the prompt
/// affordance the refreshed reading snapshot carries (`null` once it decrypts,
/// `wrong_password` to re-prompt). The passphrase stays in-process.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn submit_reading_password(
    state: State<'_, AppState>,
    account: AccountId,
    key: ProviderKey,
    passphrase: String,
) -> Option<PasswordPromptState> {
    submit_reading_password_of(&state, &account, &key, passphrase)
}

#[cfg(test)]
#[path = "crypto_tests.rs"]
mod tests;
