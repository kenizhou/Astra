// SPDX-License-Identifier: GPL-3.0-only
//! The JMAP sign-in's account half: what the discovery exchange's
//! document becomes once it answered, the JMAP parallel of
//! [`super::graph_add`] over the same recorded placement (the account
//! wiring is host-side; the dial table stands in for a registry).
//!
//! The ordering this module owns, pinned on the re-auth seam's doc and
//! proven here over a rotating token endpoint:
//!
//! 1. **The document** arrives complete: the address is the one the
//!    flow began with (the discovery targeted it), and the derived id
//!    (`address@jmap:<host>`) keys everything after it. No address
//!    lookup runs: unlike the integrated providers, the person typed
//!    the address into the form that began the flow.
//! 2. **The re-consent check** reads the dial table by that id: an
//!    entry that stands means the grant is being renewed, and the
//!    stored shape keeps what it stored beside the old grant.
//! 3. **The entry registers BEFORE the dial**: the rotation sink reads
//!    the dial table's entry, so it must stand before the dial or a
//!    rotation during the dial has nowhere to land. A ratcheting JMAP
//!    server revokes the whole grant over a rotation nobody stored, so
//!    this is the ordering the account's next launch stands on.
//! 4. **The persist reads the dial table, not the parsed copy**: the
//!    dial may already have rotated the refresh token through the sink,
//!    and writing the copy parsed a moment ago over that would restore
//!    the superseded token. A new id persists through the transaction's
//!    `add` (its own rollback on a refused store); a re-consent goes
//!    through `rotate`, never `add`, whose rollback would delete a
//!    standing account's credential.
//! 5. **The account joins the app** without syncing, and the first sync
//!    runs in the background: the same assembly the boot performs.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use engine_api::AccountId;

use astra_account::{AccountConfig, AccountError, GraphTokenSource};

use crate::app_state::{AppPort, HostAccount, HostProvider};
use crate::dial_table::AccountDials;
use crate::runtime::AppRuntime;

use super::commands::RefusalKind;
use super::grant_refusals::indeterminate;
pub(crate) use super::grant_refusals::GraphAddFailure;
use super::transaction::CredentialTransaction;

/// The host halves the completion drives: the app core the account
/// joins, the dial table the sink reads and the pre-register writes, the
/// secure store the grant persists through, the runtime handle the first
/// sync spawns on, and the standing grant refusals the dial's answer
/// records into for the surfaces.
pub(crate) struct JmapAccountWiring {
    /// The app core: the account joins it without syncing.
    pub(crate) app: Arc<dyn AppPort>,
    /// The registered accounts: the pre-register's landing and the
    /// sink's read.
    pub(crate) dials: Arc<AccountDials>,
    /// The OS secure store the grant persists through, as a transaction
    /// per write.
    pub(crate) credentials: Arc<dyn crate::credential_store::AccountCredentialStore>,
    /// The runtime handle the first sync spawns on.
    pub(crate) spawner: tokio::runtime::Handle,
    /// The standing grant refusals the surfaces read.
    pub(crate) refusals: Arc<super::grant_refusals::GrantRefusals>,
}

impl JmapAccountWiring {
    /// The wiring over one assembled state.
    #[must_use]
    pub(crate) fn from_state(state: &crate::AppState, runtime: &AppRuntime) -> Self {
        Self {
            app: Arc::clone(&state.app),
            dials: Arc::clone(&state.connections),
            credentials: Arc::clone(&state.credential_store),
            spawner: runtime.handle(),
            refusals: Arc::clone(&state.refusals),
        }
    }
}

/// The dial seam: the account's id, its shared token source, and its
/// stored config in; the one account-wide provider out. The one
/// function every path that brings an OAuth JMAP account up calls, so
/// the completion, the boot, and the folder binder cannot disagree
/// about what a JMAP account opens.
pub(crate) type JmapDial = Arc<
    dyn Fn(
            &AccountId,
            Arc<GraphTokenSource>,
            &AccountConfig,
        ) -> Pin<Box<dyn Future<Output = Result<HostProvider, AccountError>> + Send>>
        + Send
        + Sync,
>;

/// The dial through the account layer's free function: the production
/// seam, the same one the boot's restore and the folder binder dial
/// through.
#[must_use]
pub(crate) fn production_jmap_dial() -> JmapDial {
    Arc::new(|_account, tokens, config| {
        let config = config.clone();
        Box::pin(async move {
            let tls = astra_account::account_tls(&config)?;
            let provider = astra_account::dial_jmap_oauth_account(&config, &tls, &tokens).await?;
            Ok(Box::new(provider) as HostProvider)
        })
    })
}

/// Carries what an existing account stored over to the document a new
/// sign-in for it produced: the pinned id and every section the grant
/// does not own, because a re-consent changes the grant and nothing
/// else.
fn keep_stored_shape(signed_in: &mut AccountConfig, stored: Option<AccountConfig>) {
    let Some(stored) = stored else {
        return;
    };
    signed_in.shape.id = stored.shape.id;
    signed_in.dav = stored.dav;
    signed_in.certificate_exceptions = stored.certificate_exceptions;
}

/// Takes the pre-registration back: a new add's entry goes whole, a
/// re-consent's entry is put back as it stood, because a failed dial or
/// a refused store must leave a live account's standing registration
/// alone.
fn roll_back_registration(dials: &AccountDials, id: &AccountId, stored: Option<AccountConfig>) {
    match stored {
        Some(config) => dials.replace_config(id.as_str(), config),
        None => {
            if let Err(err) = dials.remove(id.as_str()) {
                log::warn!(
                    "oauth: a rolled-back account's id stayed in the dial table's index: {err}"
                );
            }
        }
    }
}

/// Completes a JMAP sign-in into an account: the document parses, the
/// dial table entry registers **before** the dial (the rotation sink
/// reads it), the dial proves the grant works and wraps the one
/// account-wide provider, the grant persists through the transaction
/// (an `add` for a new id, a `rotate` for a re-consent), and the account
/// joins the app with its first sync in the background.
///
/// # Errors
///
/// Answers [`GraphAddFailure`] for every refusal along that path; the
/// rollback leaves nothing registered for a new id and the standing
/// entry exactly as it was for a re-consent. On success the answer is
/// the address the flow began with.
pub(crate) async fn complete_jmap_signin(
    wiring: JmapAccountWiring,
    document: String,
    dial: JmapDial,
) -> Result<String, GraphAddFailure> {
    let JmapAccountWiring {
        app,
        dials,
        credentials,
        spawner,
        refusals,
    } = wiring;

    let mut config =
        astra_account::load_str(&document).map_err(|err| indeterminate(err.to_string()))?;
    let id = config
        .account_id()
        .map_err(|err| indeterminate(err.to_string()))?;
    let email = config
        .jmap
        .as_ref()
        .map(|jmap| jmap.email.clone())
        .ok_or_else(|| indeterminate("the document stores no [jmap] section".to_owned()))?;

    // Signing an existing account in again keeps what it stored beside
    // its grant, and its renewal persists through the rotation path: the
    // add path's rollback would delete a standing account's credential.
    let stored = dials.of(id.as_str()).map(|(stored, _)| stored);
    let reconsent = stored.is_some();
    keep_stored_shape(&mut config, stored.clone());

    // The sink reads the dial table's entry, so the entry must stand
    // BEFORE the dial: a rotation during the dial lands on the fresh
    // grant rather than nowhere. The same source the dial reads is the
    // one the sink advances, so there is one refresher of this
    // credential and not two.
    let sink = super::graph_sink::graph_token_sink(Arc::clone(&dials), Arc::clone(&credentials));
    let section = config
        .jmap
        .as_ref()
        .ok_or_else(|| indeterminate("the document stores no [jmap] section".to_owned()))?;
    let source = astra_account::jmap_token_source(
        section,
        id.clone(),
        Some(sink),
        astra_account::CredentialOrigin::FreshSignIn,
    )
    .map_err(|err| indeterminate(err.to_string()))?;
    if let Err(err) = dials.register(&id, config.clone(), astra_account::ImapConnections::new()) {
        roll_back_registration(&dials, &id, stored);
        return Err(indeterminate(err.to_string()));
    }

    let provider = match dial(&id, Arc::clone(&source), &config).await {
        Ok(provider) => provider,
        // A dead grant during the dial is the dead word, never an outage.
        Err(err @ AccountError::SigninRejected(_)) => {
            roll_back_registration(&dials, &id, stored);
            if !reconsent {
                // A first add whose dial failed may have rotated through
                // the sink: the credential belongs to no account, so it
                // goes with the registration.
                let _ = CredentialTransaction::new(Arc::clone(&credentials)).remove(&id);
            }
            return Err(GraphAddFailure {
                refusal: RefusalKind::Dead,
                detail: err.to_string(),
            });
        }
        Err(err) => {
            roll_back_registration(&dials, &id, stored);
            if !reconsent {
                let _ = CredentialTransaction::new(Arc::clone(&credentials)).remove(&id);
            }
            return Err(indeterminate(err.to_string()));
        }
    };

    // The grant works, so store it; read back from the dial table, not
    // the copy parsed a moment ago, because the dial may already have
    // rotated the refresh token through the sink and writing the earlier
    // copy over that would restore the superseded token.
    let transaction = CredentialTransaction::new(Arc::clone(&credentials));
    let document = match dials.of(id.as_str()) {
        Some((persisted, _)) => persisted.to_toml().map_err(|err| {
            roll_back_registration(&dials, &id, stored.clone());
            indeterminate(err.to_string())
        }),
        None => Err(indeterminate(
            "the account left the dial table while its grant was being stored".to_owned(),
        )),
    }?;
    let persist = if reconsent {
        transaction.rotate(&id, &document)
    } else {
        transaction.add(&id, &document)
    };
    if let Err(err) = persist {
        // The add path's own rollback already deleted best-effort; a
        // refused rotation leaves the previous document stored, which is
        // still what the next launch loads.
        roll_back_registration(&dials, &id, stored);
        return Err(indeterminate(err.to_string()));
    }

    // The account joins the app without syncing, and the first sync runs
    // in the background: the same assembly the boot performs. A dial
    // that answered clears whatever refusal stood.
    refusals.record_dialled(id.as_str(), false);
    let account = HostAccount {
        id: id.clone(),
        providers: vec![provider],
        identity: engine_api::EmailAddress::new(email.clone()),
    };
    let join_app = Arc::clone(&app);
    join_app.add_new_account_deferred(account).await;
    let sync_app = Arc::clone(&app);
    let sync_id = id.clone();
    spawner.spawn(async move { sync_app.sync_added_account(&sync_id).await });
    log::info!(
        "oauth: the JMAP sign-in completed into an account; the first sync runs in the background"
    );
    Ok(email)
}

#[cfg(test)]
#[path = "jmap_add_tests.rs"]
mod tests;
