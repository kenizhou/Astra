// SPDX-License-Identifier: GPL-3.0-only
//! The completion's account half: what a Microsoft sign-in's [`TokenSet`]
//! becomes once the exchange answered, ported from the reference's
//! `complete_microsoft_login` and adapted to this host's recorded placement
//! (the account wiring is host-side; there is no registry, the dial table
//! stands in for it).
//!
//! The ordering this module owns, load-bearing exactly as the reference
//! states it:
//!
//! 1. **The address lookup** names the account from the just-minted access
//!    token; the derived id (`email@graph`) keys everything after it.
//! 2. **The re-consent check** reads the dial table by that id: an entry
//!    that stands means the grant is being renewed, and the stored shape
//!    keeps what it stored beside the old grant (the pinned id, the dav
//!    legs, the accepted certificates).
//! 3. **The token source registers before the dial**: the sink reads the
//!    dial table's entry, so the entry must stand before the dial or a
//!    rotation during the dial has nowhere to land and the next launch
//!    presents a spent token. One refresher per credential.
//! 4. **The persist reads the dial table, not the parsed copy**: the dial
//!    may already have rotated the refresh token through the sink, and
//!    writing the copy parsed a moment ago over that would restore the
//!    superseded token. A new id persists through the transaction's `add`
//!    (its own rollback on a refused store); a re-consent goes through
//!    `rotate`, never `add`, whose rollback would delete a standing
//!    account's credential.
//! 5. **The account joins the app** without syncing, and the first sync
//!    runs in the background: the same assembly the boot performs.
//!
//! [`TokenSet`]: astra_oauth::TokenSet

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use astra_oauth::{GrantedScopes, OAuthProviderConfig, TokenSet};
use engine_api::{AccountId, TimeZoneId};

use astra_account::{
    AccountConfig, AccountShape, CredentialOrigin, GraphAccount, GraphTokenSource,
    MicrosoftProviders, Secret, TokenSink,
};

use crate::app_state::{AppPort, HostAccount};
use crate::dial_table::AccountDials;
use crate::runtime::AppRuntime;

use super::commands::RefusalKind;
use super::grant_refusals::indeterminate;
pub(crate) use super::grant_refusals::GraphAddFailure;
use super::transaction::CredentialTransaction;

/// The account half's success: what the wizard's success sentence shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SignedInAccount {
    /// The address the lookup named, as the account is known by.
    pub(crate) address: String,
    /// Whether the calendar was withheld for a re-consent (a scope-denied
    /// `403` on the calendar probe): the mail stands, the agenda waits for
    /// the wider grant.
    pub(crate) calendar_withheld: bool,
}

/// The host halves the completion drives: the app core the account joins,
/// the dial table the sink reads and the pre-register writes, the secure
/// store the grant persists through, the runtime handle the first sync
/// spawns on, and the standing grant refusals the dial's answer records
/// into for the surfaces.
pub(crate) struct GraphAccountWiring {
    /// The app core: the account joins it without syncing.
    pub(crate) app: Arc<dyn AppPort>,
    /// The registered accounts: the pre-register's landing and the sink's
    /// read.
    pub(crate) dials: Arc<AccountDials>,
    /// The OS secure store the grant persists through, as a transaction
    /// per write.
    pub(crate) credentials: Arc<dyn crate::credential_store::AccountCredentialStore>,
    /// The runtime handle the first sync spawns on.
    pub(crate) spawner: tokio::runtime::Handle,
    /// The standing grant refusals: what the dialled grant bears lands
    /// here, so the refusal surfaces read the newest state the same pull
    /// that reads the new account's row.
    pub(crate) refusals: Arc<super::grant_refusals::GrantRefusals>,
}

impl GraphAccountWiring {
    /// The wiring over one assembled state: the command surface builds this
    /// for the completion it hands the spawned sign-in half.
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

/// The address-lookup seam: the just-minted access token in, the account's
/// own address out.
pub(crate) type AddressLookup = Arc<
    dyn Fn(
            &str,
        )
            -> Pin<Box<dyn Future<Output = Result<String, astra_account::AccountError>> + Send>>
        + Send
        + Sync,
>;

/// The lookup over the real Graph `/me` read: the production seam.
#[must_use]
pub(crate) fn production_lookup() -> AddressLookup {
    Arc::new(|access_token| {
        let access_token = access_token.to_owned();
        Box::pin(async move { astra_account::fetch_primary_address(&access_token).await })
    })
}

/// The dial seam: the account's id, its shared token source, and its
/// stored config in; the dialled providers out. The one function every
/// path that brings a Graph account up calls, so boot, sign-in, and
/// reconnect cannot disagree about what a Microsoft account opens.
pub(crate) type GraphDial = Arc<
    dyn Fn(
            &AccountId,
            Arc<GraphTokenSource>,
            &AccountConfig,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<MicrosoftProviders, astra_account::AccountError>> + Send,
            >,
        > + Send
        + Sync,
>;

/// The dial through the account layer's free function: the production
/// seam, the same one the boot's restore dials through.
///
/// The display zone is UTC: this host carries no zone setting yet, and a
/// Graph calendar's wall clock needs one today; the settings slice that
/// owns a zone replaces this constant, not the call.
#[must_use]
pub(crate) fn production_dial() -> GraphDial {
    Arc::new(|account, tokens, config| {
        let account = account.clone();
        let config = config.clone();
        Box::pin(async move { dial_graph_account(&account, tokens, &config).await })
    })
}

/// Dials a Microsoft account's providers through the account layer's one
/// free function: the mail providers for its folders and the provider for
/// its default calendar, over the one shared token source.
async fn dial_graph_account(
    account: &AccountId,
    tokens: Arc<GraphTokenSource>,
    config: &AccountConfig,
) -> Result<MicrosoftProviders, astra_account::AccountError> {
    astra_account::dial_microsoft_account(account, tokens, config, TimeZoneId::utc()).await
}

/// The token-source seam: the provider config, the account id, the refresh
/// token, and the sink in; the shared source out.
pub(crate) type SourceBuilder = Arc<
    dyn Fn(
            OAuthProviderConfig,
            AccountId,
            String,
            Option<Arc<dyn TokenSink>>,
        ) -> Result<Arc<GraphTokenSource>, String>
        + Send
        + Sync,
>;

/// The source over the provider config the flow carried: the production
/// seam. A fresh sign-in replaces any live credential state for the
/// account, because the credential it described is gone.
#[must_use]
pub(crate) fn production_source() -> SourceBuilder {
    Arc::new(|provider, account, refresh_token, sink| {
        GraphTokenSource::new(
            &provider,
            account,
            &refresh_token,
            sink,
            CredentialOrigin::FreshSignIn,
        )
        .map_err(|err| err.to_string())
    })
}

/// Carries what an existing account stored over to the document a new
/// sign-in for it produced: the pinned id and every section the grant does
/// not own (the dav legs, the accepted certificates), because a re-consent
/// changes the grant and nothing else.
fn keep_stored_shape(signed_in: &mut AccountConfig, stored: Option<AccountConfig>) {
    let Some(stored) = stored else {
        return;
    };
    signed_in.shape.id = stored.shape.id;
    signed_in.dav = stored.dav;
    signed_in.certificate_exceptions = stored.certificate_exceptions;
}

/// The tenant the flow's endpoints authenticate against: the first path
/// segment of the token endpoint, which the provider constructor builds as
/// `{authority}/{tenant}/oauth2/v2.0/token`.
fn tenant_of(provider: &OAuthProviderConfig) -> String {
    provider
        .token_endpoint
        .trim_start_matches("https://")
        .split('/')
        .nth(1)
        .filter(|tenant| !tenant.is_empty())
        .unwrap_or("common")
        .to_owned()
}

/// Takes the pre-registration back: a new add's entry goes whole, a
/// re-consent's entry is put back as it stood, because a failed dial or a
/// refused store must leave a live account's standing registration alone.
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

/// Completes a Microsoft sign-in into an account: the address lookup names
/// it, the `[graph]` document builds from the token set, the dial runs
/// over a registered token source, the grant persists through the
/// transaction (an `add` for a new id, a `rotate` for a re-consent), and
/// the account joins the app with its first sync in the background.
///
/// # Errors
///
/// Answers [`GraphAddFailure`] for every refusal along that path; the
/// rollback leaves nothing registered for a new id and the standing entry
/// exactly as it was for a re-consent.
pub(crate) async fn complete_microsoft_signin(
    wiring: GraphAccountWiring,
    tokens: TokenSet,
    provider: OAuthProviderConfig,
    lookup: AddressLookup,
    dial: GraphDial,
    source: SourceBuilder,
) -> Result<SignedInAccount, GraphAddFailure> {
    let GraphAccountWiring {
        app,
        dials,
        credentials,
        spawner,
        refusals,
    } = wiring;

    // The address lookup names the account; the derived id keys everything.
    let address = lookup(tokens.access_token.expose())
        .await
        .map_err(|err| indeterminate(err.to_string()))?;
    // `offline_access` was requested, so a refresh token is mandatory;
    // without it the account would break an hour after setup.
    let refresh = tokens.refresh_token.as_ref().ok_or_else(|| {
        indeterminate("Microsoft issued no refresh token (offline_access scope)".to_owned())
    })?;
    let granted = GrantedScopes::from_response(&tokens.scope, &provider.scopes);
    let graph = GraphAccount {
        email: address.clone(),
        client_id: provider.client_id.clone(),
        tenant: tenant_of(&provider),
        redirect_uri: provider.redirect_uri.clone(),
        scopes: provider.scopes.clone(),
        refresh_token: Secret::new(refresh.expose().to_owned()),
        granted_scopes: Some(granted.as_slice().to_vec()),
    };
    // A grant that names no scope at all cannot bear mail: refusing it
    // here keeps the person on the setup surface they are still looking
    // at, rather than a full add/rollback cycle ending in an
    // indeterminate folder-list refusal (mail-calendar's door rule).
    if granted.as_slice().is_empty() {
        return Err(indeterminate(
            "the provider granted this account no permission at all; nothing to connect".to_owned(),
        ));
    }
    let mut config = AccountConfig {
        imap: None,
        smtp: None,
        dav: None,
        graph: Some(graph),
        google: None,
        certificate_exceptions: Vec::new(),
        shape: AccountShape::default(),
    };
    let id = config
        .account_id()
        .map_err(|err| indeterminate(err.to_string()))?;

    // Signing an existing account in again keeps what it stored beside its
    // grant, and its renewal persists through the rotation path: the add
    // path's rollback would delete a standing account's credential.
    let stored = dials.of(id.as_str()).map(|(stored, _)| stored);
    let reconsent = stored.is_some();
    keep_stored_shape(&mut config, stored.clone());

    // The sink reads the dial table's entry, so the entry must stand
    // BEFORE the dial: a rotation during the dial has somewhere to land.
    // The same source the dial reads is the one the sink advances, so
    // there is one refresher of this credential and not two.
    let sink = super::graph_sink::graph_token_sink(Arc::clone(&dials), Arc::clone(&credentials));
    let minted_access = tokens.access_token.expose().to_owned();
    let minted_expiry = tokens.expires_at;
    let source = source(
        provider,
        id.clone(),
        refresh.expose().to_owned(),
        Some(sink),
    )
    .map_err(indeterminate)?;
    // Seeded with the just-minted access token, so the first sync needs no
    // immediate refresh and a freshly signed-in account inherits no
    // cool-down from the credential it replaced.
    source.seed_access_token(minted_access, minted_expiry);
    if let Err(err) = dials.register(&id, config.clone(), astra_account::ImapConnections::new()) {
        roll_back_registration(&dials, &id, stored);
        return Err(indeterminate(err.to_string()));
    }

    let dialled = match dial(&id, Arc::clone(&source), &config).await {
        Ok(dialled) => dialled,
        // A dead grant during the dial is the dead word, never an outage.
        Err(err @ astra_account::AccountError::SigninRejected(_)) => {
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

    // The grant works, so store it; read back from the dial table, not the
    // copy built a moment ago, because the dial may already have rotated
    // the refresh token through the sink and writing the earlier copy over
    // that would restore the superseded token.
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
    // in the background: the same assembly the boot performs.
    let calendar_withheld = dialled.calendar_reauth_required;
    // What the new grant bears is the standing state the surfaces read: a
    // withheld calendar records the reconnect state, a healthy grant
    // clears whatever stood, because a dial that answered proves it wrong.
    refusals.record_dialled(id.as_str(), calendar_withheld);
    if let Some(err) = &dialled.calendar_error {
        if calendar_withheld {
            log::warn!(
                "oauth: the calendar was withheld from this sign-in; a fresh one re-grants it: {err}"
            );
        } else {
            log::warn!("oauth: the calendar could not be connected; the mail stands: {err}");
        }
    }
    let mut providers = dialled.mail;
    if let Some(calendar) = dialled.calendar {
        providers.push(calendar);
    }
    let account = HostAccount {
        id: id.clone(),
        providers,
        identity: engine_api::EmailAddress::new(address.clone()),
    };
    let join_app = Arc::clone(&app);
    join_app.add_new_account_deferred(account).await;
    let sync_app = Arc::clone(&app);
    let sync_id = id.clone();
    spawner.spawn(async move { sync_app.sync_added_account(&sync_id).await });
    log::info!(
        "oauth: the Microsoft sign-in completed into an account; the first sync runs in the \
         background"
    );
    Ok(SignedInAccount {
        address,
        calendar_withheld,
    })
}

/// The completion the spawned sign-in half runs for a token set: the
/// future it answers, boxed once so the wiring can name it.
pub(crate) type SignInCompletion = Box<
    dyn FnOnce(
            TokenSet,
            OAuthProviderConfig,
        ) -> Pin<Box<dyn Future<Output = super::commands::SignInOutcome> + Send>>
        + Send,
>;

/// The completion a begun flow's success runs, wired for one provider: a
/// Microsoft sign-in becomes the real add over the production seams;
/// every other provider answers the bare signed-in word until its slice
/// lands.
#[must_use]
pub(crate) fn outcome_completion(
    state: &crate::AppState,
    runtime: &crate::runtime::AppRuntime,
    provider: super::commands::ProviderKind,
) -> SignInCompletion {
    use super::commands::{ProviderKind, SignInOutcome};
    match provider {
        ProviderKind::Microsoft => {
            let wiring = GraphAccountWiring::from_state(state, runtime);
            let lookup = production_lookup();
            let dial = production_dial();
            let source = production_source();
            Box::new(move |tokens, provider_config| {
                Box::pin(async move {
                    match complete_microsoft_signin(
                        wiring,
                        tokens,
                        provider_config,
                        lookup,
                        dial,
                        source,
                    )
                    .await
                    {
                        Ok(added) => SignInOutcome::Added {
                            address: added.address,
                            calendar_withheld: added.calendar_withheld,
                        },
                        Err(failure) => SignInOutcome::Refused {
                            refusal: failure.refusal,
                            detail: failure.detail,
                        },
                    }
                })
            })
        }
        ProviderKind::Google => Box::new(move |tokens, _provider_config| {
            Box::pin(async move {
                let _ = tokens;
                SignInOutcome::SignedIn
            })
        }),
    }
}

#[cfg(test)]
#[path = "graph_add_tests.rs"]
mod tests;
