// SPDX-License-Identifier: GPL-3.0-only
//! The host's AI seams: the app's completion and judge traits over the
//! background client, and the incoming-pass assembly the boot wires over
//! them. The completion adapter is the honest failure path (the app layer
//! reports the host's failure as its message); the judge adapter is the
//! infallible seam the matching paths rest on (the client folds every
//! failure into the use case's safe default before it answers, so no error
//! ever crosses).

use std::sync::Arc;

use astra_ai::{
    AiStore, CorrelationId, DlpError, DlpPolicy, JudgeOutcome, JudgeUseCase, LlmUseCase,
};
use astra_app::{AiComplete, AiJudge, AiPass};
use async_trait::async_trait;

use super::LlmClient;

/// The completion seam over the background client: one ledgered completion
/// per call, the tier chain and the per-attempt rows all the client's own.
pub struct LlmAiComplete {
    /// The client every call rides.
    client: Arc<LlmClient>,
}

impl core::fmt::Debug for LlmAiComplete {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The client may hold provider credentials; show nothing of them.
        f.debug_struct("LlmAiComplete").finish_non_exhaustive()
    }
}

impl LlmAiComplete {
    /// Wraps `client`.
    #[must_use]
    pub fn new(client: Arc<LlmClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl AiComplete for LlmAiComplete {
    async fn complete(
        &self,
        use_case: LlmUseCase,
        prompt: &str,
        correlation: &CorrelationId,
    ) -> Result<String, String> {
        self.client
            .complete(use_case, prompt, correlation)
            .await
            .map_err(|error| error.to_string())
    }
}

/// The judge seam over the background client: one ledgered binary judge
/// call with the spend guard, the retry, and the safe-default fold all the
/// client's own. This adapter adds nothing and blocks nothing: it exists so
/// the app's [`AiJudge`] trait objects over the host's client, the seam the
/// incoming pass and the tests share.
pub struct LlmAiJudge {
    /// The client every call rides.
    client: Arc<LlmClient>,
}

impl core::fmt::Debug for LlmAiJudge {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The client may hold provider credentials; show nothing of them.
        f.debug_struct("LlmAiJudge").finish_non_exhaustive()
    }
}

impl LlmAiJudge {
    /// Wraps `client`.
    #[must_use]
    pub fn new(client: Arc<LlmClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl AiJudge for LlmAiJudge {
    async fn judge(
        &self,
        use_case: JudgeUseCase,
        question: &str,
        context: &str,
        correlation: &CorrelationId,
    ) -> JudgeOutcome {
        self.client
            .judge(use_case, question, context, correlation)
            .await
    }
}

/// Assembles the incoming pass the app core runs: both seams over the one
/// background client, the one AI store every path shares, and the DLP
/// policy the gate applies. No client (the tier table or the transport
/// never built) means no pass, and so does a malformed policy value: a
/// misconfigured `ASTRA_DLP` fails the pass's arming loudly rather than
/// silently allowing, the recorded decision.
pub fn incoming_pass(
    client: Option<Arc<LlmClient>>,
    store: Arc<AiStore>,
    dlp: Result<DlpPolicy, DlpError>,
) -> Option<AiPass> {
    let policy = match dlp {
        Ok(policy) => policy,
        Err(error) => {
            log::error!(
                "boot: the dlp policy is unusable; the incoming ai pass stays unarmed: {error}"
            );
            return None;
        }
    };
    let client = client?;
    Some(AiPass::new(
        Box::new(LlmAiJudge::new(Arc::clone(&client))),
        Box::new(LlmAiComplete::new(Arc::clone(&client))),
        store,
        policy,
    ))
}

#[cfg(test)]
#[path = "seams_tests.rs"]
mod tests;
