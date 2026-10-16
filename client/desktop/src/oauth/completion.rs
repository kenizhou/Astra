// SPDX-License-Identifier: GPL-3.0-only
//! The completion dispatch: the one begun flow's success runs the account
//! half its provider names. Every provider this build offers completes
//! into an account (Microsoft through [`super::graph_add`], Google
//! through [`super::google_add`]), over each family's own production
//! seams; a provider no account consumes keeps answering the bare
//! signed-in word until its slice lands.

use std::future::Future;
use std::pin::Pin;

use astra_oauth::{OAuthProviderConfig, TokenSet};

use super::commands::{ProviderKind, SignInOutcome};

/// The completion a spawned sign-in half runs for a token set: the
/// future it answers, boxed once so the wiring can name it.
pub(crate) type SignInCompletion = Box<
    dyn FnOnce(TokenSet, OAuthProviderConfig) -> Pin<Box<dyn Future<Output = SignInOutcome> + Send>>
        + Send,
>;

/// The completion a begun flow's success runs, wired for one provider:
/// a Microsoft sign-in becomes the real add over the graph seams, a
/// Google one over the google seams.
#[must_use]
pub(crate) fn outcome_completion(
    state: &crate::AppState,
    runtime: &crate::runtime::AppRuntime,
    provider: ProviderKind,
) -> SignInCompletion {
    match provider {
        ProviderKind::Microsoft => {
            let wiring = super::graph_add::GraphAccountWiring::from_state(state, runtime);
            let lookup = super::graph_add::production_lookup();
            let dial = super::graph_add::production_dial();
            let source = super::graph_add::production_source();
            Box::new(move |tokens, provider_config| {
                Box::pin(async move {
                    match super::graph_add::complete_microsoft_signin(
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
        ProviderKind::Google => {
            let wiring = super::google_add::GoogleAccountWiring::from_state(state, runtime);
            let lookup = super::google_add::production_google_lookup();
            let dial = super::google_add::production_google_dial();
            let source = super::google_add::production_google_source();
            Box::new(move |tokens, provider_config| {
                Box::pin(async move {
                    match super::google_add::complete_google_signin(
                        wiring,
                        tokens,
                        provider_config,
                        lookup,
                        dial,
                        source,
                    )
                    .await
                    {
                        // The calendar never withholds for a Google
                        // account: the scope is asked for at sign-in, so
                        // the reconnect banner stays Microsoft-only.
                        Ok(address) => SignInOutcome::Added {
                            address,
                            calendar_withheld: false,
                        },
                        Err(failure) => SignInOutcome::Refused {
                            refusal: failure.refusal,
                            detail: failure.detail,
                        },
                    }
                })
            })
        }
    }
}
