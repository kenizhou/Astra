// SPDX-License-Identifier: GPL-3.0-only
//! Judge tests: the strict parser (case, punctuation, the pinned abstain
//! marker, garbage), the retry-once state machine, the safe-default table
//! pin (a judge use case without a default fails), the versioned
//! single-token prompt form, and the wire words of the judge types.

use crate::judge::{
    judge_prompt, parse_judgement, safe_default, JudgeDefault, JudgeOutcome, JudgePath,
    JudgeUseCase, JudgeVerdict, ABSTAIN_MARKER, JUDGE_TEMPLATES_VERSION, SAFE_DEFAULTS,
};
use crate::{LlmUseCase, ModelTier};

/// Every judge use case, exactly once, in declaration order. A new variant
/// that is not added here fails the table-coverage pin below (and a variant
/// without a `safe_default` arm fails compilation first).
const ALL_JUDGE_USE_CASES: [JudgeUseCase; 7] = [
    JudgeUseCase::JudgeColdEmail,
    JudgeUseCase::JudgeRuleMatch,
    JudgeUseCase::JudgeDlp,
    JudgeUseCase::JudgeInjection,
    JudgeUseCase::JudgeDraftSurface,
    JudgeUseCase::JudgeSummaryFaithful,
    JudgeUseCase::JudgeScenario,
];

#[test]
fn yes_and_no_parse_case_insensitively_after_trimmed_punctuation() {
    for (raw, verdict) in [
        ("yes", JudgeVerdict::Yes),
        ("Yes", JudgeVerdict::Yes),
        ("YES", JudgeVerdict::Yes),
        ("  yes  ", JudgeVerdict::Yes),
        ("yes.", JudgeVerdict::Yes),
        ("no!", JudgeVerdict::No),
        ("NO", JudgeVerdict::No),
        (" No?", JudgeVerdict::No),
        ("\"yes\"", JudgeVerdict::Yes),
        ("'no'", JudgeVerdict::No),
        ("**yes**", JudgeVerdict::Yes),
        ("no;", JudgeVerdict::No),
        ("yes,", JudgeVerdict::Yes),
    ] {
        assert_eq!(parse_judgement(raw), verdict, "raw answer {raw:?}");
    }
}

#[test]
fn the_abstain_marker_is_the_pinned_word_case_insensitive() {
    assert_eq!(ABSTAIN_MARKER, "abstain");
    for raw in ["abstain", "Abstain", "ABSTAIN", " abstain.", "abstain?"] {
        assert_eq!(parse_judgement(raw), JudgeVerdict::Abstain, "raw {raw:?}");
    }
}

#[test]
fn anything_else_parses_to_abstain() {
    for raw in [
        "",
        "   ",
        "maybe",
        "y",
        "n",
        "yes no",
        "Yes, I think so",
        "no comment",
        "\u{201c}yes\u{201d}",
        "true",
        "0",
    ] {
        assert_eq!(parse_judgement(raw), JudgeVerdict::Abstain, "raw {raw:?}");
    }
}

#[test]
fn a_decisive_first_try_stands_without_consulting_the_retry() {
    for (first, retried) in [
        (JudgeVerdict::Yes, JudgeVerdict::No),
        (JudgeVerdict::No, JudgeVerdict::Yes),
        (JudgeVerdict::Yes, JudgeVerdict::Abstain),
    ] {
        assert_eq!(
            JudgeOutcome::resolve(first, retried, JudgeVerdict::No),
            JudgeOutcome {
                verdict: first,
                path: JudgePath::FirstTry
            },
            "first {first:?} stands on its own",
        );
    }
}

#[test]
fn a_decisive_retry_wins_with_the_retry_path() {
    for retried in [JudgeVerdict::Yes, JudgeVerdict::No] {
        assert_eq!(
            JudgeOutcome::resolve(JudgeVerdict::Abstain, retried, JudgeVerdict::No),
            JudgeOutcome {
                verdict: retried,
                path: JudgePath::Retry
            },
            "retried {retried:?}",
        );
    }
}

#[test]
fn a_retry_that_still_abstains_resolves_to_the_safe_default() {
    assert_eq!(
        JudgeOutcome::resolve(
            JudgeVerdict::Abstain,
            JudgeVerdict::Abstain,
            JudgeVerdict::No
        ),
        JudgeOutcome {
            verdict: JudgeVerdict::No,
            path: JudgePath::AbstainDefault
        },
    );
}

#[test]
fn the_safe_default_table_pins_the_seven_rows() {
    assert_eq!(
        SAFE_DEFAULTS,
        [
            (JudgeUseCase::JudgeColdEmail, JudgeDefault::No),
            (JudgeUseCase::JudgeRuleMatch, JudgeDefault::No),
            (JudgeUseCase::JudgeDlp, JudgeDefault::Block),
            (JudgeUseCase::JudgeInjection, JudgeDefault::Hostile),
            (JudgeUseCase::JudgeDraftSurface, JudgeDefault::No),
            (JudgeUseCase::JudgeSummaryFaithful, JudgeDefault::Fail),
            (JudgeUseCase::JudgeScenario, JudgeDefault::Fail),
        ],
        "the judge spec's table, verbatim",
    );
}

#[test]
fn every_judge_use_case_has_exactly_one_default() {
    assert_eq!(
        SAFE_DEFAULTS.len(),
        ALL_JUDGE_USE_CASES.len(),
        "the table and the use-case set agree on size",
    );
    for use_case in ALL_JUDGE_USE_CASES {
        let rows = SAFE_DEFAULTS
            .iter()
            .filter(|(table_case, _)| *table_case == use_case)
            .count();
        assert_eq!(rows, 1, "{use_case} has exactly one table row");
    }
}

#[test]
fn safe_default_lookup_agrees_with_the_table_for_every_use_case() {
    for (use_case, expected) in SAFE_DEFAULTS {
        assert_eq!(safe_default(use_case), expected, "{use_case}");
    }
}

#[test]
fn every_safe_default_resolves_to_the_no_pole() {
    for default in [
        JudgeDefault::No,
        JudgeDefault::Block,
        JudgeDefault::Hostile,
        JudgeDefault::Fail,
    ] {
        assert_eq!(default.verdict(), JudgeVerdict::No, "{default:?}");
    }
}

#[test]
fn judge_use_cases_map_to_their_llm_use_cases_and_tiers() {
    let cases = [
        (
            JudgeUseCase::JudgeColdEmail,
            LlmUseCase::JudgeColdEmail,
            ModelTier::Nano,
        ),
        (
            JudgeUseCase::JudgeRuleMatch,
            LlmUseCase::JudgeRuleMatch,
            ModelTier::Economy,
        ),
        (
            JudgeUseCase::JudgeDlp,
            LlmUseCase::JudgeDlp,
            ModelTier::Nano,
        ),
        (
            JudgeUseCase::JudgeInjection,
            LlmUseCase::JudgeInjection,
            ModelTier::Nano,
        ),
        (
            JudgeUseCase::JudgeDraftSurface,
            LlmUseCase::JudgeDraftSurface,
            ModelTier::Nano,
        ),
        (
            JudgeUseCase::JudgeSummaryFaithful,
            LlmUseCase::JudgeSummaryFaithful,
            ModelTier::Economy,
        ),
        (
            JudgeUseCase::JudgeScenario,
            LlmUseCase::JudgeScenario,
            ModelTier::Economy,
        ),
    ];
    for (judge_case, llm_case, tier) in cases {
        assert_eq!(judge_case.llm_use_case(), llm_case);
        assert_eq!(judge_case.tier(), tier);
        assert_eq!(judge_case.llm_use_case().tier(), tier);
    }
}

#[test]
fn the_judge_wire_words_are_snake_case() {
    let verdicts = [
        (JudgeVerdict::Yes, "\"yes\""),
        (JudgeVerdict::No, "\"no\""),
        (JudgeVerdict::Abstain, "\"abstain\""),
    ];
    for (verdict, wire) in verdicts {
        assert_eq!(serde_json::to_string(&verdict).unwrap(), wire);
    }
    let paths = [
        (JudgePath::FirstTry, "\"first_try\""),
        (JudgePath::Retry, "\"retry\""),
        (JudgePath::AbstainDefault, "\"abstain_default\""),
    ];
    for (path, wire) in paths {
        assert_eq!(serde_json::to_string(&path).unwrap(), wire);
    }
    let use_cases = [
        (JudgeUseCase::JudgeColdEmail, "\"judge_cold_email\""),
        (JudgeUseCase::JudgeRuleMatch, "\"judge_rule_match\""),
        (JudgeUseCase::JudgeDlp, "\"judge_dlp\""),
        (JudgeUseCase::JudgeInjection, "\"judge_injection\""),
        (JudgeUseCase::JudgeDraftSurface, "\"judge_draft_surface\""),
        (
            JudgeUseCase::JudgeSummaryFaithful,
            "\"judge_summary_faithful\"",
        ),
        (JudgeUseCase::JudgeScenario, "\"judge_scenario\""),
    ];
    for (use_case, wire) in use_cases {
        assert_eq!(serde_json::to_string(&use_case).unwrap(), wire);
        assert_eq!(
            serde_json::to_string(&use_case.llm_use_case()).unwrap(),
            wire,
            "the judge use case and its LLM use case share the wire word",
        );
    }
}

#[test]
fn judge_outcomes_round_trip_through_serde() {
    let outcome = JudgeOutcome {
        verdict: JudgeVerdict::No,
        path: JudgePath::AbstainDefault,
    };
    let wire = serde_json::to_string(&outcome).unwrap();
    assert_eq!(wire, r#"{"verdict":"no","path":"abstain_default"}"#);
    let back: JudgeOutcome = serde_json::from_str(&wire).unwrap();
    assert_eq!(back, outcome);
}

#[test]
fn the_judge_prompt_pins_the_versioned_single_token_form() {
    let prompt = judge_prompt(
        "Is this message cold outreach?",
        "Buy now, discount inside.",
    );
    assert_eq!(
        prompt,
        "Is this message cold outreach?\n\
         \n\
         <untrusted heading=\"judge context\">\n\
         [[[untrusted:begin]]]\n\
         Buy now, discount inside.\n\
         [[[untrusted:end]]]\n\
         </untrusted>\n\
         \n\
         Answer with exactly one word: yes, no, or abstain.",
        "question, hardening-wrapped context, then the strict instruction",
    );
}

#[test]
fn already_wrapped_context_is_not_double_wrapped() {
    let wrapped = crate::tag_untrusted("email body", "Buy now");
    let prompt = judge_prompt("Is this cold?", &wrapped);
    assert!(
        prompt.contains("<untrusted heading=\"email body\">") && !prompt.contains("judge context"),
        "the judge template reuses the standing wrapper unchanged: {prompt}",
    );
}

#[test]
fn judge_templates_version_is_pinned() {
    assert_eq!(JUDGE_TEMPLATES_VERSION, "1");
}
