// SPDX-License-Identifier: GPL-3.0-only
//! AI pass fixtures: the judge seam fake and the app builder that injects
//! it. A child of the fixture root, its own file so that root stays under
//! the 500-line limit; every item is `pub(crate)` so the AI test modules
//! path it directly through `dispatch_fakes::ai`.

use std::sync::{Arc, Mutex};

use astra_ai::{AiStore, CorrelationId, JudgeOutcome, JudgeUseCase};
use async_trait::async_trait;

use crate::ai_rules::AiJudge;
use crate::{Account, App, Surface};

use super::{app as base_app, FakeProvider};

/// One judge call the fixture received, frozen for the assertions: the use
/// case, the question half, the context half, and the correlation, exactly
/// as the seam carried them.
#[derive(Clone)]
pub(crate) struct JudgeCall {
    /// The judge family member asked.
    pub(crate) use_case: JudgeUseCase,
    /// The trusted instruction half (the question and its rule condition).
    pub(crate) question: String,
    /// The untrusted half (the hardening-wrapped mail content).
    pub(crate) context: String,
    /// The turn the call served.
    pub(crate) correlation: CorrelationId,
}

/// The shared heart of a [`FixtureJudge`], behind an `Arc` so a test can
/// hold a probe beside the copy the app runs.
struct JudgeInner {
    answers: Mutex<Vec<JudgeOutcome>>,
    repeat: bool,
    calls: Mutex<Vec<JudgeCall>>,
}

/// A judge seam that answers from a canned script and records every call.
/// Cloning hands the app and the test the same heart, so the probe reads
/// the calls the app made. A strict script runs out loudly: an
/// under-scripted test panics rather than silently reusing an answer.
pub(crate) struct FixtureJudge {
    inner: Arc<JudgeInner>,
}

impl Clone for FixtureJudge {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl FixtureJudge {
    /// A seam that answers the given outcomes in order, and panics when the
    /// script runs out.
    pub(crate) fn scripted(outcomes: &[JudgeOutcome]) -> Self {
        Self::built(outcomes.to_vec(), false)
    }

    /// A seam that answers `outcome` for every call: the volume tests'
    /// shape, where the call count is the assertion.
    pub(crate) fn answering(outcome: JudgeOutcome) -> Self {
        Self::built(vec![outcome], true)
    }

    /// The one constructor both canned shapes come from.
    fn built(answers: Vec<JudgeOutcome>, repeat: bool) -> Self {
        Self {
            inner: Arc::new(JudgeInner {
                answers: Mutex::new(answers),
                repeat,
                calls: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Every call received so far, cloned out for the assertions.
    pub(crate) fn calls(&self) -> Vec<JudgeCall> {
        self.inner
            .calls
            .lock()
            .expect("fixture judge calls unpoisoned")
            .clone()
    }

    /// How many calls the seam has received.
    pub(crate) fn call_count(&self) -> usize {
        self.inner
            .calls
            .lock()
            .expect("fixture judge calls unpoisoned")
            .len()
    }
}

#[async_trait]
impl AiJudge for FixtureJudge {
    async fn judge(
        &self,
        use_case: JudgeUseCase,
        question: &str,
        context: &str,
        correlation: &CorrelationId,
    ) -> JudgeOutcome {
        let answer = {
            let mut answers = self
                .inner
                .answers
                .lock()
                .expect("fixture judge answers unpoisoned");
            if self.inner.repeat {
                answers[0]
            } else {
                answers.remove(0)
            }
        };
        self.inner
            .calls
            .lock()
            .expect("fixture judge calls unpoisoned")
            .push(JudgeCall {
                use_case,
                question: question.to_owned(),
                context: context.to_owned(),
                correlation: correlation.clone(),
            });
        answer
    }
}

/// Builds an in-memory app over `accounts` whose AI pass runs through
/// `judge` and records into `store`, the shape the desktop host assembles
/// over its real client and the one store.
pub(crate) fn app_with_ai(
    accounts: Vec<Account<FakeProvider>>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
    judge: FixtureJudge,
    store: Arc<AiStore>,
) -> App<FakeProvider> {
    base_app(accounts, surfaces).with_ai_pass(Some(crate::AiPass::new(Box::new(judge), store)))
}
