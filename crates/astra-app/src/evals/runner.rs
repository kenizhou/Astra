// SPDX-License-Identifier: GPL-3.0-only
//! The scenario runner: one scenario driven through the seams it names,
//! the same fixture injection the unit tests use ([`FixtureComplete`],
//! [`FixtureJudge`]), and the same stores and builders the incoming pass
//! drives. Offline scenarios carry their canned seam answers in
//! `inputs` and run fully deterministic; a live caller (the Task 6
//! suite's client adapter) passes its own seam objects to [`drive`],
//! where the model's answers replace the fixtures and every expectation
//! must still hold.

use astra_ai::{redact, AiStore, CorrelationId, JudgeOutcome, JudgePath, JudgeVerdict, RuleId};
use engine_api::{AccountId, ProviderKey};

use crate::ai_rules::{
    categorise, judge_cold, match_rules, prompt_to_rules, Action, AiComplete, AiJudge, MailContext,
    Rule,
};
use crate::dispatch_fakes::ai::{FixtureComplete, FixtureJudge};

use super::{Feature, Scenario};

/// Runs one offline scenario: the canned seam answers its inputs carry,
/// injected through the crate's own fakes, with the outcome compared in
/// the feature's own shape.
pub(crate) async fn offline(scenario: &Scenario) -> Result<(), String> {
    let completion = scenario
        .inputs
        .get("completion")
        .and_then(serde_json::Value::as_str);
    let complete = completion.map_or_else(
        || FixtureComplete::scripted(&[]),
        |answer| FixtureComplete::scripted(&[answer]),
    );
    let judge = outcome_field(&scenario.inputs)?.map_or_else(
        || FixtureJudge::scripted(&[]),
        |outcome| FixtureJudge::scripted(&[outcome]),
    );
    drive(scenario, &complete, &judge).await
}

/// Drives one scenario over the given seams, whatever backs them: the
/// offline fakes or a live client adapter. The scenario's text, mail, and
/// rule go in; the recorded outcome comes out and meets the expectation.
pub(crate) async fn drive(
    scenario: &Scenario,
    complete: &dyn AiComplete,
    judge: &dyn AiJudge,
) -> Result<(), String> {
    let store = AiStore::open_in_memory().expect("the scenario store opens");
    let correlation = CorrelationId::generate();
    match scenario.feature {
        Feature::Generation => {
            let text = str_field(&scenario.inputs, "text")?;
            let actual = match prompt_to_rules(&text, &correlation, complete).await {
                Ok(rules) => serde_json::json!({
                    "rules": rules.iter().map(rule_json).collect::<Vec<_>>()
                }),
                Err(err) => serde_json::json!({ "error": error_word(&err.to_string()) }),
            };
            expect(&scenario.expected, &actual)
        }
        Feature::Categorise => {
            let mail = mail_context(&scenario.inputs)?;
            let actual = match categorise(&mail, &correlation, complete, &store).await {
                Ok(category) => {
                    serde_json::json!({ "category": category.as_str(), "verdict": "ok" })
                }
                Err(_) => serde_json::json!({ "category": null, "verdict": "skipped" }),
            };
            expect(&scenario.expected, &actual)
        }
        Feature::Cold => {
            let mail = mail_context(&scenario.inputs)?;
            judge_cold(&mail, &correlation, judge, &store).await;
            let verdict = newest_row(&store, &mail, "cold")?;
            expect(
                &scenario.expected,
                &serde_json::json!({ "verdict": verdict }),
            )
        }
        Feature::Match => {
            let rule = rule_field(&scenario.inputs)?;
            let mail = mail_context(&scenario.inputs)?;
            let matched = match_rules(&mail, &[rule], &correlation, judge, &store).await;
            let verdict = newest_row(&store, &mail, "rule_match")?;
            let names = matched
                .iter()
                .map(|result| result.rule.name.clone())
                .collect::<Vec<_>>();
            expect(
                &scenario.expected,
                &serde_json::json!({ "matched": names, "verdict": verdict }),
            )
        }
        Feature::JudgeTemplate => judge_template(scenario),
        Feature::Redaction => redaction(scenario),
    }
}

/// The judge template scenario: the pure builder's shape, the single
/// token instruction present and the context inside the untrusted fences.
fn judge_template(scenario: &Scenario) -> Result<(), String> {
    let question = str_field(&scenario.inputs, "question")?;
    let context = str_field(&scenario.inputs, "context")?;
    let prompt = astra_ai::judge_prompt(&question, &context);
    for needle in strings_field(&scenario.expected, "contains")? {
        if !prompt.contains(&needle) {
            return Err(format!("the template dropped {needle:?}:\n{prompt}"));
        }
    }
    if bool_field(&scenario.expected, "context_wrapped")? {
        let begin = prompt
            .find("[[[untrusted:begin]]]")
            .ok_or_else(|| "the wrap never opens".to_owned())?;
        let end = prompt
            .find("[[[untrusted:end]]]")
            .ok_or_else(|| "the wrap never closes".to_owned())?;
        let at = prompt
            .find(&context)
            .ok_or_else(|| "the context never rides".to_owned())?;
        if !(begin < at && at < end) {
            return Err(format!("the context rode outside the fences:\n{prompt}"));
        }
    }
    Ok(())
}

/// The redaction scenario: the pure redactor's exact output, corners
/// included; a fixture that documents an unmasked corner pins the input
/// unchanged as the expectation.
fn redaction(scenario: &Scenario) -> Result<(), String> {
    let content = str_field(&scenario.inputs, "content")?;
    let expected = str_field(&scenario.expected, "redacted")?;
    let actual = redact(&content);
    if actual == expected {
        Ok(())
    } else {
        Err(format!("redaction moved:\nin:  {content}\nout: {actual}"))
    }
}

/// One rule as the generation expectation compares it: identity aside
/// (every parse mints a fresh one), the fields the scenario pins.
fn rule_json(rule: &Rule) -> serde_json::Value {
    serde_json::json!({
        "name": rule.name,
        "condition": rule.condition,
        "actions": rule.actions.iter().map(Action::as_str).collect::<Vec<_>>(),
        "enabled": rule.enabled,
    })
}

/// The failure word for a generation outcome the scenario expects to
/// fail: the completion seam's failure or the unparseable output.
fn error_word(message: &str) -> String {
    if message.starts_with("rules generation call failed") {
        "completion".to_owned()
    } else {
        "malformed".to_owned()
    }
}

/// The scenario's mail context, built from the `mail` object the fixture
/// carries.
fn mail_context(inputs: &serde_json::Value) -> Result<MailContext, String> {
    let mail = inputs
        .get("mail")
        .ok_or_else(|| "carries no mail object".to_owned())?;
    let field = |name: &str| {
        mail.get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("the mail object carries no {name}"))
    };
    Ok(MailContext {
        account: AccountId::try_from(field("account")?.as_str()).map_err(|err| err.to_string())?,
        key: ProviderKey::new(field("key")?).map_err(|err| err.to_string())?,
        from_name: field("from_name")?,
        from_address: field("from_address")?,
        subject: field("subject")?,
        body: field("body")?,
    })
}

/// The scenario's rule, minted a fresh identity the way generation does.
fn rule_field(inputs: &serde_json::Value) -> Result<Rule, String> {
    let rule = inputs
        .get("rule")
        .ok_or_else(|| "carries no rule object".to_owned())?;
    let name = str_field(rule, "name")?;
    let condition = str_field(rule, "condition")?;
    let mut actions = Vec::new();
    for word in rule
        .get("actions")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "the rule object carries no actions".to_owned())?
    {
        let word = word
            .as_str()
            .ok_or_else(|| "the rule's actions must be words".to_owned())?;
        let action = Action::ALL
            .iter()
            .find(|action| action.as_str() == word)
            .copied()
            .ok_or_else(|| format!("the rule names action {word:?} outside the closed set"))?;
        actions.push(action);
    }
    Ok(Rule {
        id: RuleId::generate(),
        name,
        condition,
        actions,
        enabled: true,
    })
}

/// The judge outcome a fixture scripts, from the `outcome` object's
/// verdict and path words.
fn outcome_field(inputs: &serde_json::Value) -> Result<Option<JudgeOutcome>, String> {
    let Some(outcome) = inputs.get("outcome") else {
        return Ok(None);
    };
    let verdict = |word: &str| match word {
        "yes" => Ok(JudgeVerdict::Yes),
        "no" => Ok(JudgeVerdict::No),
        "abstain" => Ok(JudgeVerdict::Abstain),
        _ => Err(format!("unknown verdict word {word:?}")),
    };
    let path = |word: &str| match word {
        "first_try" => Ok(JudgePath::FirstTry),
        "retry" => Ok(JudgePath::Retry),
        "abstain_default" => Ok(JudgePath::AbstainDefault),
        _ => Err(format!("unknown path word {word:?}")),
    };
    Ok(Some(JudgeOutcome {
        verdict: verdict(str_field(outcome, "verdict")?.as_str())?,
        path: path(str_field(outcome, "path")?.as_str())?,
    }))
}

/// The verdict word the newest row of `stage` carries for the mail, from
/// the store the stage recorded into: the read that proves the results
/// row landed beside the verdict.
fn newest_row(store: &AiStore, mail: &MailContext, stage: &str) -> Result<String, String> {
    let rows = store
        .ai_results_for_message(mail.account.as_str(), mail.key.as_str())
        .map_err(|err| format!("the results read failed: {err}"))?;
    rows.iter()
        .find(|row| row.stage == stage)
        .map(|row| row.verdict.clone())
        .ok_or_else(|| format!("no {stage} row was recorded"))
}

/// A required string field.
fn str_field(value: &serde_json::Value, name: &str) -> Result<String, String> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("carries no {name} string"))
}

/// A required array-of-strings field.
fn strings_field(value: &serde_json::Value, name: &str) -> Result<Vec<String>, String> {
    value
        .get(name)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("carries no {name} array"))?
        .iter()
        .map(|word| {
            word.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("the {name} array must carry strings"))
        })
        .collect()
}

/// A required boolean field.
fn bool_field(value: &serde_json::Value, name: &str) -> Result<bool, String> {
    value
        .get(name)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| format!("carries no {name} boolean"))
}

/// Structural equality against the expectation, both sides printed on a
/// mismatch.
fn expect(expected: &serde_json::Value, actual: &serde_json::Value) -> Result<(), String> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "the outcome moved:\nexpected: {expected}\nactual:   {actual}"
        ))
    }
}
