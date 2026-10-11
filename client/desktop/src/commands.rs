// SPDX-License-Identifier: GPL-3.0-only
//! The IPC command surface: the `#[tauri::command]` handlers the webview invokes,
//! and the plain functions under them the tests drive. Every handler is a thin
//! wrapper over [`AppState`]; the logic lives in the inner functions so the seam is
//! exercised without a webview.
//!
//! The shape follows the unidirectional loop: `dispatch` takes an intent as JSON and
//! hands it to the app on the runtime; `mailbox_list`, `sync_progress` and `reading`
//! pull the immutable snapshots; `add_account` and `imap_test_connection` are the two
//! account-setup verbs, the only commands that dial.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use tauri::State;

use astra_account::ImapConnections;
use astra_viewmodel::{MailboxListSnapshot, ReadingSnapshot, SyncProgressSnapshot};
use engine_api::AccountId;

use crate::{AppState, HostAccount, HostProvider};

// The compose command surface (the request pull), split into its own child
// module to keep this file under the size limit, the same split the app core's
// protocol made for its compose intents.
pub(crate) mod compose;

// The drafts command surface (the status pull and the resume read), under the
// same split.
pub(crate) mod drafts;

// The sending command surface (the queue pull), under the same split.
pub(crate) mod sending;

// The AI replies command surface (the chips, summarise, style, and dismiss
// pulls), under the same split.
pub(crate) mod ai;

// The AI documents command surface (the digest and report generations,
// the latest read, and the split map), under the same split.
pub(crate) mod ai_docs;

// The calendar command surface (the snapshot pull and the meeting brief
// generation), under the same split.
pub(crate) mod calendar;

/// A command failure, serialised to the webview as its display string: a sentence a
/// surface can show, with the reason the host saw.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CommandError {
    /// The intent could not be read from the JSON the webview sent.
    #[error("the intent could not be read: {0}")]
    Intent(String),
    /// The account config could not be parsed or is incomplete.
    #[error("the account config could not be read: {0}")]
    Config(String),
    /// The account's server could not be connected or the sign-in was refused.
    #[error("the account could not be connected: {0}")]
    Connect(String),
    /// The OS secure store refused the credential write.
    #[error("the credential store refused: {0}")]
    Store(String),
    /// The account index could not be written.
    #[error("the account index could not be written: {0}")]
    Index(String),
    /// The settings-detection run could not produce a recommendation.
    #[error("the settings could not be detected: {0}")]
    Detect(String),
    /// The agent sidecar is unavailable or refused the request.
    #[error("the agent is unavailable: {0}")]
    Agent(String),
    /// The AI replies surface is unconfigured or refused the request.
    #[error("the ai surface refused: {0}")]
    Ai(String),
}

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CommandError {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(CommandError::Intent(String::deserialize(deserializer)?))
    }
}

/// The record an added account returns: the identity a setup flow keys its stored
/// config and its switcher row with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AddedAccount {
    /// The account's stable id (derived from its login).
    pub account_id: String,
    /// The address the account sends and is shown as.
    pub email: String,
}

/// What the rejected certificate claims about itself, for the UI to show beside the
/// fingerprint so a person can recognise a server they meant to reach. Every field
/// is `None` when the bytes did not parse, which an unvalidated certificate is
/// entitled to be.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CertificateClaims {
    /// The name the certificate claims to be (`CN`).
    pub subject_common_name: Option<String>,
    /// The organisation it claims to belong to (`O`).
    pub subject_organisation: Option<String>,
    /// Who issued it (`CN`); equal to the subject when it signed itself.
    pub issuer_common_name: Option<String>,
    /// The organisation that issued it (`O`).
    pub issuer_organisation: Option<String>,
    /// When it claims to become valid, in seconds since the Unix epoch.
    pub not_before: Option<i64>,
    /// When it claims to expire, in seconds since the Unix epoch.
    pub not_after: Option<i64>,
}

/// A server certificate that did not verify, as the UI shows it and hands it back:
/// the server name and fingerprint an accepted exception is scoped to, beside the
/// certificate's own claims. A user who accepts it passes the pair back on the
/// setup's `accepted_certificate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RejectedCertificateView {
    /// The TLS server name that was asked for; the exception is scoped to it.
    pub server_name: String,
    /// The certificate's SHA-256, uppercase and colon-separated, as every tool shows
    /// a fingerprint and as somebody would compare it against their own server.
    pub sha256: String,
    /// The name the certificate claims to be (`CN`).
    pub subject_common_name: Option<String>,
    /// The organisation it claims to belong to (`O`).
    pub subject_organisation: Option<String>,
    /// Who issued it (`CN`); equal to the subject when it signed itself.
    pub issuer_common_name: Option<String>,
    /// The organisation that issued it (`O`).
    pub issuer_organisation: Option<String>,
    /// When it claims to become valid, in seconds since the Unix epoch.
    pub not_before: Option<i64>,
    /// When it claims to expire, in seconds since the Unix epoch.
    pub not_after: Option<i64>,
}

/// Builds the view of a rejected certificate from the parts the engine's rejection
/// exposes: the server name, the fingerprint, and the parsed claims when there are
/// any. Split out so the mapping is testable without a TLS server: the engine type
/// itself only exists after a real refusal.
pub(crate) fn map_rejected(
    server_name: &str,
    fingerprint: &[u8; 32],
    claims: Option<CertificateClaims>,
) -> RejectedCertificateView {
    let mut sha256 = String::with_capacity(32 * 3);
    for (at, byte) in fingerprint.iter().enumerate() {
        if at > 0 {
            sha256.push(':');
        }
        let _ = write!(sha256, "{byte:02X}");
    }
    let claims = claims.unwrap_or_default();
    RejectedCertificateView {
        server_name: server_name.to_owned(),
        sha256,
        subject_common_name: claims.subject_common_name,
        subject_organisation: claims.subject_organisation,
        issuer_common_name: claims.issuer_common_name,
        issuer_organisation: claims.issuer_organisation,
        not_before: claims.not_before,
        not_after: claims.not_after,
    }
}

/// The verdict of one setup connection test, as the setup form reads it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ImapTestOutcome {
    /// The dial connected and logged in; the setup can proceed to `add_account`.
    Ok {
        /// The account id the config derives.
        account_id: String,
    },
    /// The server presented a certificate that did not verify: the one failure a
    /// person can settle without changing anything about the account. The UI shows
    /// `certificate` and may offer to accept it for this one server, passing the
    /// pair back on the setup's `accepted_certificate`.
    RejectedCertificate {
        /// The certificate as the UI shows it.
        certificate: RejectedCertificateView,
        /// What the transport said, kept for the diagnostic log.
        reason: String,
    },
    /// The connection or the sign-in failed outright.
    Failed {
        /// The failure as a sentence.
        message: String,
    },
}

/// Parses one intent from the JSON the webview sent and hands it to the app on the
/// runtime. The intent family's outcome reaches the surfaces, not the return value:
/// this returns once the intent is dispatched, and an unparsable payload is a typed
/// error, never a panic.
pub(crate) fn dispatch_intent(
    state: &AppState,
    intent: serde_json::Value,
) -> Result<(), CommandError> {
    let intent: astra_app::Intent =
        serde_json::from_value(intent).map_err(|err| CommandError::Intent(err.to_string()))?;
    let app = std::sync::Arc::clone(&state.app);
    state.runtime.spawn(async move {
        if let Err(err) = app.dispatch(intent).await {
            log::warn!("dispatch: an intent failed: {err}");
        }
    });
    Ok(())
}

/// The published mailbox-list snapshot.
pub(crate) fn mailbox_list_of(state: &AppState) -> MailboxListSnapshot {
    state.app.mailbox_list()
}

/// The published sync-progress snapshot.
pub(crate) fn sync_progress_of(state: &AppState) -> SyncProgressSnapshot {
    state.app.sync_progress()
}

/// The published reading snapshot: the one open message's sanitised body, or `None`
/// when nothing is open.
pub(crate) fn reading_of(state: &AppState) -> Option<ReadingSnapshot> {
    state.app.reading()
}

/// Adds one account, the whole transaction: parse the config, derive the id, dial
/// through `dial`, **persist once the dial succeeds** (a refused write rolls the
/// whole add back: nothing is registered), register the account's connections, hand
/// the account to the app without syncing, and spawn its first sync on the runtime.
///
/// `dial` is injected so the flow is testable without a server: production blocks on
/// the real connect through the runtime's driver thread, a test answers from a
/// closure. The dial returns the account's connection pool beside its providers,
/// because the pool the providers drew on is the one later binds must draw on too.
pub(crate) fn add_account_with<D>(
    state: &AppState,
    config_toml: &str,
    dial: D,
) -> Result<AddedAccount, CommandError>
where
    D: FnOnce(
        &astra_account::AccountConfig,
        &AccountId,
    ) -> Result<
        (Vec<HostProvider>, std::sync::Arc<ImapConnections>),
        astra_account::AccountError,
    >,
{
    let config = astra_account::load_str(config_toml)
        .map_err(|err| CommandError::Config(err.to_string()))?;
    let id = config
        .account_id()
        .map_err(|err| CommandError::Config(err.to_string()))?;
    let email = config
        .imap
        .as_ref()
        .map(|imap| imap.username.clone())
        .ok_or_else(|| CommandError::Config("the account names no mail server".to_owned()))?;

    let (providers, connections) =
        dial(&config, &id).map_err(|err| CommandError::Connect(err.to_string()))?;

    // Persist only now that the dial succeeded, and treat a refusal as the end of the
    // add: an account that works until the next launch and then silently is not there
    // is worse than a visible "could not save this account". Nothing has been
    // registered yet, so there is nothing to un-register.
    state
        .credential_store
        .persist(id.as_str(), config_toml)
        .map_err(|err| CommandError::Store(err.to_string()))?;

    let account = HostAccount {
        id: id.clone(),
        providers,
        identity: engine_api::EmailAddress::new(email.clone()),
    };
    if let Err(err) = state.connections.register(&id, config, connections) {
        // The credential outlived the registration: take it back rather than leave an
        // entry the next boot's index never names.
        let _ = state.credential_store.delete(id.as_str());
        return Err(CommandError::Index(err.to_string()));
    }

    let app = std::sync::Arc::clone(&state.app);
    state
        .runtime
        .block_on(|| async { app.add_new_account_deferred(account).await });
    let sync_app = std::sync::Arc::clone(&state.app);
    let sync_id = id.clone();
    state
        .runtime
        .spawn(async move { sync_app.sync_added_account(&sync_id).await });

    log::info!(
        "add-account: connected, stored and registered; first sync running in the background",
    );
    Ok(AddedAccount {
        account_id: id.as_str().to_owned(),
        email,
    })
}

/// Runs one setup connection test: builds the config the setup's fields describe,
/// dials once through `dial`, and maps the outcome, carrying a certificate rejection
/// out as the record the UI offers an exception from.
pub(crate) fn imap_test_with<D>(
    setup: serde_json::Value,
    dial: D,
) -> Result<ImapTestOutcome, CommandError>
where
    D: FnOnce(&astra_account::AccountConfig) -> Result<(), astra_account::AccountError>,
{
    let setup: astra_account::AccountSetup =
        serde_json::from_value(setup).map_err(|err| CommandError::Config(err.to_string()))?;
    let config_toml = astra_account::build_config_toml(&setup)
        .map_err(|err| CommandError::Config(err.to_string()))?;
    let config = astra_account::load_str(&config_toml)
        .map_err(|err| CommandError::Config(err.to_string()))?;
    let id = config
        .account_id()
        .map_err(|err| CommandError::Config(err.to_string()))?;

    Ok(match dial(&config) {
        Ok(()) => ImapTestOutcome::Ok {
            account_id: id.as_str().to_owned(),
        },
        Err(astra_account::AccountError::CertificateRejected { reason, rejected }) => {
            ImapTestOutcome::RejectedCertificate {
                certificate: map_rejected(
                    rejected.server_name(),
                    &rejected.fingerprint(),
                    rejected.summary().map(|summary| CertificateClaims {
                        subject_common_name: summary.subject_common_name().map(str::to_owned),
                        subject_organisation: summary.subject_organization().map(str::to_owned),
                        issuer_common_name: summary.issuer_common_name().map(str::to_owned),
                        issuer_organisation: summary.issuer_organization().map(str::to_owned),
                        not_before: Some(summary.not_before()),
                        not_after: Some(summary.not_after()),
                    }),
                ),
                reason,
            }
        }
        Err(err) => ImapTestOutcome::Failed {
            message: err.to_string(),
        },
    })
}

/// Dispatches one intent, as JSON from the webview, onto the app's runtime.
///
/// # Errors
///
/// Returns [`CommandError::Intent`] when the payload is not an intent; a dispatched
/// intent's outcome arrives through the surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn dispatch(
    state: State<'_, AppState>,
    intent: serde_json::Value,
) -> Result<(), CommandError> {
    dispatch_intent(&state, intent)
}

/// Pulls the mailbox-list snapshot.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn mailbox_list(state: State<'_, AppState>) -> MailboxListSnapshot {
    mailbox_list_of(&state)
}

/// Pulls the sync-progress snapshot.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn sync_progress(state: State<'_, AppState>) -> SyncProgressSnapshot {
    sync_progress_of(&state)
}

/// Pulls the reading snapshot: the open message's body, or null when the pane is
/// closed. Reading is signalled through on every publish, so this is pulled per
/// signal rather than on a debounce.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn reading(state: State<'_, AppState>) -> Option<ReadingSnapshot> {
    reading_of(&state)
}

/// Adds one account from its config TOML: dials, persists, registers, and spawns the
/// first sync. The dial blocks on the runtime's driver thread, so the command waits
/// for it and the setup form learns the outcome.
///
/// # Errors
///
/// Returns [`CommandError`] when the config cannot be read, the dial fails, or the
/// store refuses the write; a refused store write leaves nothing registered.
// Tauri's command macro hands `State` by value; the wrapper reads and dials through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) async fn add_account(
    state: State<'_, AppState>,
    config_toml: String,
) -> Result<AddedAccount, CommandError> {
    let dial = |config: &astra_account::AccountConfig, id: &AccountId| {
        let connections = ImapConnections::new();
        let providers = state
            .runtime
            .block_on(|| astra_account::connect_mail_providers(&connections, config, id));
        providers.map(|providers| (providers, connections))
    };
    add_account_with(&state, &config_toml, dial)
}

/// Tests one setup's connection once: reports success, a rejected certificate the UI
/// may offer to accept, or the failure itself.
///
/// # Errors
///
/// Returns [`CommandError::Config`] when the setup fields cannot be read; the
/// connection's own outcome is the [`ImapTestOutcome`] success value.
// Tauri's command macro hands `State` by value; the wrapper reads and dials through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) async fn imap_test_connection(
    state: State<'_, AppState>,
    setup: serde_json::Value,
) -> Result<ImapTestOutcome, CommandError> {
    imap_test_with(setup, |config| {
        state.runtime.block_on(|| async {
            astra_account::connect_mail_providers(
                &ImapConnections::new(),
                config,
                &config
                    .account_id()
                    .map_err(|err| astra_account::AccountError::Mailbox(err.to_string()))?,
            )
            .await
            .map(|_| ())
        })
    })
}

/// Detects the mail-server settings for `email`: the email-first setup step. Runs
/// one bounded detection on the runtime with the host's DNS resolver, then maps
/// the result onto the recommendation view. Never panics; every failure is the
/// typed [`CommandError::Detect`] the wizard falls back to manual from.
///
/// # Errors
///
/// Returns [`CommandError::Detect`] when the address has no usable domain, the
/// trust store cannot be built, or the run outlives its five second bound.
// Tauri's command macro hands `State` by value; the wrapper reads and dials through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) async fn detect_account_settings(
    state: State<'_, AppState>,
    email: String,
) -> Result<crate::detection::RecommendationView, CommandError> {
    let resolver = crate::detection::detection_resolver(state.runtime.handle());
    let config = crate::detection::detect_config();
    crate::detection::detect_settings_with(
        &state.runtime,
        &email,
        crate::detection::DETECT_BOUND,
        || async { astra_autodetect::detect(&email, resolver, &config).await },
    )
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
