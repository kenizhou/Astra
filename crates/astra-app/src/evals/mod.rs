// SPDX-License-Identifier: GPL-3.0-only
//! The eval harness: the scenario fixtures under the repository's
//! `evals/` directory, the vocabulary that parses them, and the
//! seam-parameter drive that runs one scenario over any seam pair.
//! Deterministic scenarios (fixture completions and judgements injected
//! through the crate's own fakes) run as ordinary offline tests in CI;
//! judge-backed live scenarios dial a model endpoint, so they run under
//! the `ASTRA_LIVE_LLM` gate through the drive's seam parameters, driven
//! by the desktop host's live suite (the only crate that holds the
//! background client).
//!
//! Every scenario carries the pipeline constant value its prompt text was
//! written against, and the offline test pins each one against the
//! constant the build carries today: bumping a prompt constant without
//! rerunning and refreshing its scenarios fails the offline suite, the
//! observable-bump net the rulebook's AI rule demands.

mod drive;

#[cfg(test)]
mod runner;

use std::path::PathBuf;

use astra_ai::JUDGE_TEMPLATES_VERSION;

use crate::ai_rules::{
    CATEGORISE_PROMPT_VERSION, COLD_JUDGE_VERSION, RULES_MATCH_VERSION, RULES_PROMPT_VERSION,
};

pub use drive::drive;

/// The environment variable that opens the live gate: judge-backed
/// scenarios dial a model endpoint, so they never run in the offline CI
/// battery.
#[cfg(test)]
const LIVE_VAR: &str = "ASTRA_LIVE_LLM";

/// The repository's `evals/` directory, resolved from this crate's
/// manifest so the test binary finds it from any working directory.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../evals")
}

/// One scenario fixture: the feature it exercises, the mode it runs in,
/// the pipeline constant value its prompt text was written against, its
/// inputs, and the outcome it expects. Fields beyond what a feature's
/// runner checks (a corner fixture's explanation, say) are documentation
/// the harness carries but does not assert.
#[derive(Debug)]
pub struct Scenario {
    /// The fixture's identity, the failure messages' name.
    pub id: String,
    /// The pipeline feature the scenario drives.
    pub feature: Feature,
    /// Whether the scenario runs offline on fixtures or live under the
    /// gate.
    pub mode: Mode,
    /// The pipeline constant value the scenario was written against;
    /// `None` only for the features that pin no prompt text.
    pub prompt_version: Option<String>,
    /// The scenario's given world: the text, the mail, and the canned
    /// seam answers an offline run injects.
    pub inputs: serde_json::Value,
    /// The outcome the scenario expects, in the feature's own shape.
    pub expected: serde_json::Value,
}

/// The pipeline features the harness drives, each naming the seam or the
/// pure builder its scenarios exercise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// Rule generation through the completion seam.
    Generation,
    /// Sender categorisation through the completion seam.
    Categorise,
    /// The cold judge through the judge seam.
    Cold,
    /// Rule matching through the judge seam.
    Match,
    /// The judge prompt template's shape, the pure builder.
    JudgeTemplate,
    /// The redactor's masked output, the pure function. No prompt text
    /// rides, so no version is pinned.
    Redaction,
}

impl Feature {
    /// The feature word from the fixture, rejected loudly when unknown.
    fn from_word(word: &str) -> Result<Self, String> {
        match word {
            "generation" => Ok(Self::Generation),
            "categorise" => Ok(Self::Categorise),
            "cold" => Ok(Self::Cold),
            "match" => Ok(Self::Match),
            "judge_template" => Ok(Self::JudgeTemplate),
            "redaction" => Ok(Self::Redaction),
            _ => Err(format!("unknown feature {word:?}")),
        }
    }

    /// The pipeline constant the feature's prompt text is pinned by, with
    /// its spelling for the failure message; `None` for the redaction
    /// feature, which pins output shapes instead of prompt text.
    fn version_pin(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Generation => Some((RULES_PROMPT_VERSION, "RULES_PROMPT_VERSION")),
            Self::Categorise => Some((CATEGORISE_PROMPT_VERSION, "CATEGORISE_PROMPT_VERSION")),
            Self::Cold => Some((COLD_JUDGE_VERSION, "COLD_JUDGE_VERSION")),
            Self::Match => Some((RULES_MATCH_VERSION, "RULES_MATCH_VERSION")),
            Self::JudgeTemplate => Some((JUDGE_TEMPLATES_VERSION, "JUDGE_TEMPLATES_VERSION")),
            Self::Redaction => None,
        }
    }
}

/// The mode a scenario runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Fixture-driven, deterministic, part of the offline battery.
    Offline,
    /// Judge-backed against the live endpoint, under the live gate.
    Live,
}

impl Mode {
    /// The mode word from the fixture, rejected loudly when unknown.
    fn from_word(word: &str) -> Result<Self, String> {
        match word {
            "offline" => Ok(Self::Offline),
            "live" => Ok(Self::Live),
            _ => Err(format!("unknown mode {word:?}")),
        }
    }
}

/// Every scenario fixture in the directory, file name order for
/// deterministic runs. A fixture that fails to parse fails the load
/// loudly: the harness never silently drops a scenario.
pub fn scenarios() -> Vec<Scenario> {
    let dir = fixtures_dir();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|err| {
            panic!(
                "the evals directory could not be read ({}): {err}",
                dir.display()
            )
        })
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let path = dir.join(&name);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("{name} could not be read: {err}"));
            let raw: serde_json::Value = serde_json::from_str(&text)
                .unwrap_or_else(|err| panic!("{name} is not valid JSON: {err}"));
            let feature = Feature::from_word(
                raw.get("feature")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("{name} carries no feature word")),
            )
            .unwrap_or_else(|err| panic!("{name}: {err}"));
            let mode = Mode::from_word(
                raw.get("mode")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("{name} carries no mode word")),
            )
            .unwrap_or_else(|err| panic!("{name}: {err}"));
            Scenario {
                id: raw
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("{name} carries no id"))
                    .to_owned(),
                feature,
                mode,
                prompt_version: raw
                    .get("prompt_version")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                inputs: raw
                    .get("inputs")
                    .cloned()
                    .unwrap_or_else(|| panic!("{name} carries no inputs")),
                expected: raw
                    .get("expected")
                    .cloned()
                    .unwrap_or_else(|| panic!("{name} carries no expected")),
            }
        })
        .collect()
}

/// The version pin: the scenario's recorded constant value must equal the
/// one the build carries. A mismatch names the constant, the fixture's
/// value, and the build's, so a bump that skipped its eval rerun is
/// readable straight off the failure.
///
/// # Errors
/// The pin failure, naming the constant and both values.
pub fn pin_version(scenario: &Scenario) -> Result<(), String> {
    match (scenario.feature.version_pin(), &scenario.prompt_version) {
        (Some((current, spelling)), Some(written)) => {
            if written == current {
                Ok(())
            } else {
                Err(format!(
                    "written against {spelling}={written} but the build carries {current}; \
                     rerun the eval and refresh the fixture"
                ))
            }
        }
        (Some((_, spelling)), None) => Err(format!(
            "carries no {spelling} value; every prompt scenario pins the constant it was written against"
        )),
        (None, Some(_)) => Err(
            "carries a prompt_version but the feature pins no prompt text; drop the field"
                .to_owned(),
        ),
        (None, None) => Ok(()),
    }
}

#[tokio::test]
async fn offline_scenarios_run_green_and_every_scenario_pins_its_version() {
    let all = scenarios();
    assert!(!all.is_empty(), "the evals directory must hold scenarios");
    let mut offline = 0usize;
    for scenario in &all {
        pin_version(scenario)
            .unwrap_or_else(|err| panic!("{}: the version pin: {err}", scenario.id));
        if scenario.mode == Mode::Offline {
            offline += 1;
            runner::offline(scenario)
                .await
                .unwrap_or_else(|err| panic!("{}: {err}", scenario.id));
        }
    }
    assert!(offline > 0, "the offline battery must not be empty");
}

#[test]
#[ignore = "live evals dial the model endpoint: run under ASTRA_LIVE_LLM=1 with the live client"]
fn live_scenarios_pin_their_versions_under_the_live_gate() {
    if std::env::var(LIVE_VAR).ok().as_deref() != Some("1") {
        eprintln!("skipping live evals: {LIVE_VAR} is not 1");
        return;
    }
    // The version pin holds for live scenarios too; the drives themselves
    // run in the desktop host's live suite (client/desktop/src/ai), the
    // crate that holds the background client the seam parameters take.
    let all = scenarios();
    let live: Vec<&Scenario> = all
        .iter()
        .filter(|scenario| scenario.mode == Mode::Live)
        .collect();
    for scenario in &live {
        pin_version(scenario)
            .unwrap_or_else(|err| panic!("{}: the version pin: {err}", scenario.id));
    }
    eprintln!(
        "{} live scenario(s) pinned; the desktop live suite drives them under {LIVE_VAR}=1",
        live.len()
    );
}
