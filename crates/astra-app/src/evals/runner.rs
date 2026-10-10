// SPDX-License-Identifier: GPL-3.0-only
//! The offline runner: one scenario driven with the canned seam answers
//! its `inputs` carry, injected through the crate's own fakes, over the
//! shared [`drive`](super::drive::drive). Test-only: the fakes are the
//! dispatch-loop fixtures.

use crate::dispatch_fakes::ai::{FixtureComplete, FixtureJudge};

use super::drive::{drive, scripted_answers};
use super::Scenario;

/// Runs one offline scenario: the canned seam answers its inputs carry,
/// injected through the crate's own fakes, with the outcome compared in
/// the feature's own shape.
pub(super) async fn offline(scenario: &Scenario) -> Result<(), String> {
    let (completion, outcome) = scripted_answers(&scenario.inputs)?;
    let complete = completion.map_or_else(
        || FixtureComplete::scripted(&[]),
        |answer| FixtureComplete::scripted(&[answer.as_str()]),
    );
    let judge = outcome.map_or_else(
        || FixtureJudge::scripted(&[]),
        |outcome| FixtureJudge::scripted(&[outcome]),
    );
    drive(scenario, &complete, &judge).await
}
