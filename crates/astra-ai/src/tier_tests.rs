// SPDX-License-Identifier: GPL-3.0-only
//! Tier table tests: env-list parsing (single, chain, malformed loud),
//! per-tier fallback to the default chain, and the SPA0 use-case mapping.

use std::collections::HashMap;

use crate::{LlmUseCase, ModelTier, TierTable};

/// Builds an environment lookup from `(name, value)` pairs; a name absent
/// from the list answers `None`, exactly like an unset variable.
fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = vars
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn single_entry_list_resolves_one_model() {
    let table = TierTable::from_lookup(lookup(&[("ASTRA_LLM_DEFAULT", "openai:gpt-6-luna")]))
        .expect("single entry parses");
    let chain = table
        .resolve(LlmUseCase::Summarise)
        .expect("summarise resolves");
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].provider, "openai");
    assert_eq!(chain[0].model, "gpt-6-luna");
}

#[test]
fn comma_chain_is_ordered_first_primary_rest_fallback() {
    let table = TierTable::from_lookup(lookup(&[(
        "ASTRA_LLM_DEFAULT",
        "anthropic:claude-sonnet-5, openai:gpt-6-luna,,groq:llama-3.3-70b-versatile",
    )]))
    .expect("chain parses");
    let chain = table
        .resolve(LlmUseCase::CleanInbox)
        .expect("clean_inbox resolves");
    let pairs: Vec<(&str, &str)> = chain
        .iter()
        .map(|entry| (entry.provider.as_str(), entry.model.as_str()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("anthropic", "claude-sonnet-5"),
            ("openai", "gpt-6-luna"),
            ("groq", "llama-3.3-70b-versatile"),
        ]
    );
}

#[test]
fn provider_is_lowercased_and_first_colon_splits() {
    let table = TierTable::from_lookup(lookup(&[("ASTRA_LLM_CHAT", "OpenAI:org/deploy:v1")]))
        .expect("entry parses");
    let chain = table
        .chain_for_tier(ModelTier::Chat)
        .expect("chat resolves");
    assert_eq!(chain[0].provider, "openai");
    assert_eq!(chain[0].model, "org/deploy:v1");
}

#[test]
fn entry_without_a_model_is_loud() {
    let err = TierTable::from_lookup(lookup(&[("ASTRA_LLM_DEFAULT", "openai")]))
        .expect_err("an entry without provider:model must fail loudly");
    let message = err.to_string();
    assert!(message.contains("ASTRA_LLM_DEFAULT"), "{message}");
    assert!(message.contains("openai"), "{message}");
}

#[test]
fn entry_with_an_empty_model_is_loud() {
    let err = TierTable::from_lookup(lookup(&[("ASTRA_LLM_DEFAULT", "openai: ")]))
        .expect_err("an empty model must fail loudly");
    assert!(err.to_string().contains("ASTRA_LLM_DEFAULT"), "{err}");
}

#[test]
fn entry_with_an_empty_provider_is_loud() {
    let err = TierTable::from_lookup(lookup(&[("ASTRA_LLM_DEFAULT", ":gpt-6-luna")]))
        .expect_err("an empty provider must fail loudly");
    assert!(err.to_string().contains("ASTRA_LLM_DEFAULT"), "{err}");
}

#[test]
fn separators_without_entries_are_loud() {
    let err = TierTable::from_lookup(lookup(&[("ASTRA_LLM_DEFAULT", ",, ,")]))
        .expect_err("a non-blank list with zero entries must fail loudly");
    assert!(err.to_string().contains("ASTRA_LLM_DEFAULT"), "{err}");
}

#[test]
fn blank_value_leaves_the_tier_unconfigured() {
    let table = TierTable::from_lookup(lookup(&[
        ("ASTRA_LLM_DEFAULT", "openai:gpt-6-luna"),
        ("ASTRA_LLM_CHAT", "  "),
    ]))
    .expect("a blank value reads as unconfigured, not malformed");
    let chain = table
        .resolve(LlmUseCase::AssistantChat)
        .expect("assistant_chat resolves via the default chain");
    assert_eq!(chain[0].model, "gpt-6-luna");
}

#[test]
fn unset_tiers_fall_back_to_default() {
    let table = TierTable::from_lookup(lookup(&[("ASTRA_LLM_DEFAULT", "openai:gpt-6-luna")]))
        .expect("parses");
    for tier in [
        ModelTier::Economy,
        ModelTier::Chat,
        ModelTier::Draft,
        ModelTier::Nano,
    ] {
        let chain = table
            .chain_for_tier(tier)
            .unwrap_or_else(|e| panic!("{tier} falls back to default: {e}"));
        assert_eq!(chain[0].model, "gpt-6-luna", "{tier}");
    }
}

#[test]
fn nano_falls_back_to_economy_before_default() {
    let table = TierTable::from_lookup(lookup(&[
        ("ASTRA_LLM_DEFAULT", "openai:gpt-6-luna"),
        ("ASTRA_LLM_ECONOMY", "groq:llama-3.3-70b-versatile"),
    ]))
    .expect("parses");
    let chain = table
        .chain_for_tier(ModelTier::Nano)
        .expect("nano resolves via economy");
    assert_eq!(chain[0].provider, "groq");
}

#[test]
fn unresolved_use_case_names_the_env_vars() {
    let table = TierTable::from_lookup(lookup(&[])).expect("empty table parses");
    let err = table
        .resolve(LlmUseCase::DraftReply)
        .expect_err("an unresolved use case must fail loudly");
    let message = err.to_string();
    assert!(message.contains("draft_reply"), "{message}");
    assert!(message.contains("ASTRA_LLM_DRAFT"), "{message}");
    assert!(message.contains("ASTRA_LLM_DEFAULT"), "{message}");
}

#[test]
fn every_tier_names_its_env_var_and_wire_word() {
    let cases = [
        (ModelTier::Default, "ASTRA_LLM_DEFAULT", "\"default\""),
        (ModelTier::Economy, "ASTRA_LLM_ECONOMY", "\"economy\""),
        (ModelTier::Chat, "ASTRA_LLM_CHAT", "\"chat\""),
        (ModelTier::Draft, "ASTRA_LLM_DRAFT", "\"draft\""),
        (ModelTier::Nano, "ASTRA_LLM_NANO", "\"nano\""),
    ];
    for (tier, var, wire) in cases {
        assert_eq!(tier.env_var(), var);
        assert_eq!(serde_json::to_string(&tier).unwrap(), wire);
    }
}

#[test]
fn spa0_use_cases_map_to_their_tiers_and_wire_words() {
    let cases = [
        (LlmUseCase::AssistantChat, ModelTier::Chat, "assistant_chat"),
        (LlmUseCase::DraftReply, ModelTier::Draft, "draft_reply"),
        (LlmUseCase::Summarise, ModelTier::Default, "summarise"),
        (
            LlmUseCase::CategorizeSender,
            ModelTier::Default,
            "categorize_sender",
        ),
        (LlmUseCase::CleanInbox, ModelTier::Default, "clean_inbox"),
        (LlmUseCase::AgentTool, ModelTier::Economy, "agent_tool"),
    ];
    for (use_case, tier, wire) in cases {
        assert_eq!(use_case.tier(), tier);
        assert_eq!(
            serde_json::to_string(&use_case).unwrap(),
            format!("\"{wire}\"")
        );
    }
}

#[test]
fn prompt_to_rules_pins_the_default_tier_and_its_wire_word() {
    assert_eq!(LlmUseCase::PromptToRules.tier(), ModelTier::Default);
    assert_eq!(
        serde_json::to_string(&LlmUseCase::PromptToRules).unwrap(),
        "\"prompt_to_rules\"",
    );
    assert_eq!(LlmUseCase::PromptToRules.as_str(), "prompt_to_rules");
}

#[test]
fn style_infer_pins_the_economy_tier_and_its_wire_word() {
    assert_eq!(LlmUseCase::StyleInfer.tier(), ModelTier::Economy);
    assert_eq!(
        serde_json::to_string(&LlmUseCase::StyleInfer).unwrap(),
        "\"style_infer\"",
    );
    assert_eq!(LlmUseCase::StyleInfer.as_str(), "style_infer");
}

#[test]
fn a3_use_cases_pin_their_tiers_and_wire_words() {
    let cases = [
        (
            LlmUseCase::KnowledgeExtract,
            ModelTier::Economy,
            "knowledge_extract",
        ),
        (
            LlmUseCase::SnippetSearch,
            ModelTier::Economy,
            "snippet_search",
        ),
        (LlmUseCase::Autocomplete, ModelTier::Chat, "autocomplete"),
        (LlmUseCase::Translate, ModelTier::Default, "translate"),
    ];
    for (use_case, tier, wire) in cases {
        assert_eq!(use_case.tier(), tier);
        assert_eq!(
            serde_json::to_string(&use_case).unwrap(),
            format!("\"{wire}\"")
        );
        assert_eq!(use_case.as_str(), wire);
    }
}

#[test]
fn a4_use_cases_pin_their_tiers_and_wire_words() {
    let cases = [
        (LlmUseCase::Digest, ModelTier::Default, "digest"),
        (LlmUseCase::Report, ModelTier::Default, "report"),
    ];
    for (use_case, tier, wire) in cases {
        assert_eq!(use_case.tier(), tier);
        assert_eq!(
            serde_json::to_string(&use_case).unwrap(),
            format!("\"{wire}\"")
        );
        assert_eq!(use_case.as_str(), wire);
    }
}
