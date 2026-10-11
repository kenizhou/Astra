// SPDX-License-Identifier: GPL-3.0-only
//! The use-case registry: why one LLM call is being made.

use serde::{Deserialize, Serialize};

use crate::tier::ModelTier;

/// Why one LLM call is being made. SPA0 ships six use cases and each AI
/// slice grows the set; the naming and the use-case to tier mapping are
/// ported from inbox-zero's use-case vocabulary
/// (`apps/web/utils/llms/use-cases.ts`). The wire words are snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmUseCase {
    /// The assistant chat loop (inbox-zero `assistant-chat`, tier chat).
    AssistantChat,
    /// Drafting a reply (inbox-zero `draft-reply`, tier draft).
    DraftReply,
    /// Summarising a message (inbox-zero `summarise`, tier default).
    Summarise,
    /// Sender categorisation (inbox-zero `categorize-sender`, tier default).
    CategorizeSender,
    /// Inbox cleaning suggestions (inbox-zero `clean-inbox`, tier default).
    CleanInbox,
    /// Generating rules from a prompt (inbox-zero `prompt-to-rules`; the
    /// A1 design pins the default tier where inbox-zero rides chat).
    PromptToRules,
    /// The agent tool loop. Inbox-zero maps its agent use cases (the
    /// `McpAgent` line) to the economy tier; `agent_tool` follows.
    AgentTool,
    /// The cold-mail judge (the binary judge spec; nano tier). Abstains
    /// resolve to not cold: the mail stays put.
    JudgeColdEmail,
    /// The rule-match judge (economy tier). Abstains resolve to no match:
    /// the rule does not run.
    JudgeRuleMatch,
    /// The data loss prevention judge (nano tier). Abstains resolve to
    /// block: privacy errs shut.
    JudgeDlp,
    /// The prompt-injection judge (nano tier). Abstains resolve to
    /// hostile: security errs closed.
    JudgeInjection,
    /// The draft-surfacing judge (nano tier; A2). Abstains resolve to
    /// unsurfaced: the draft still exists in the folder.
    JudgeDraftSurface,
    /// The summary faithfulness eval judge (economy tier). Abstains
    /// resolve to a failed eval.
    JudgeSummaryFaithful,
    /// The scenario eval judge (economy tier). Abstains resolve to a
    /// failed scenario.
    JudgeScenario,
    /// The per-account writing style inference (economy tier; A2): the
    /// StyleInfer completion that turns recent Sent mail into the style
    /// note the reply drafting prompt carries as data.
    StyleInfer,
    /// The batch knowledge extraction (economy tier; A3): the structured
    /// pass over an account's recent mail that lands stored knowledge
    /// entries with their sources tracked.
    KnowledgeExtract,
    /// The snippet library's search (economy tier; A3): the one ranking
    /// call over titles and bodies the composer's picker rides, with a
    /// substring fallback when it cannot run.
    SnippetSearch,
    /// The compose autocomplete (chat tier for latency; A3): the short
    /// continuation the editor offers for the text around the cursor.
    Autocomplete,
    /// The reading pane's translate action (default tier; A3): the open
    /// message rendered into the chosen target language.
    Translate,
    /// The on-demand digest generation (default tier; A4): the one
    /// summarising call over the window's category rollup.
    Digest,
    /// The analytics report generation (default tier; A4): the one
    /// structured call over the window's computed statistics.
    Report,
    /// The meeting brief generation (economy tier; SP2h): the one
    /// summarising call over a civil day's meetings and their linked
    /// invitation context.
    MeetingBrief,
}

impl LlmUseCase {
    /// The snake_case wire word.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AssistantChat => "assistant_chat",
            Self::DraftReply => "draft_reply",
            Self::Summarise => "summarise",
            Self::CategorizeSender => "categorize_sender",
            Self::CleanInbox => "clean_inbox",
            Self::PromptToRules => "prompt_to_rules",
            Self::AgentTool => "agent_tool",
            Self::JudgeColdEmail => "judge_cold_email",
            Self::JudgeRuleMatch => "judge_rule_match",
            Self::JudgeDlp => "judge_dlp",
            Self::JudgeInjection => "judge_injection",
            Self::JudgeDraftSurface => "judge_draft_surface",
            Self::JudgeSummaryFaithful => "judge_summary_faithful",
            Self::JudgeScenario => "judge_scenario",
            Self::StyleInfer => "style_infer",
            Self::KnowledgeExtract => "knowledge_extract",
            Self::SnippetSearch => "snippet_search",
            Self::Autocomplete => "autocomplete",
            Self::Translate => "translate",
            Self::Digest => "digest",
            Self::Report => "report",
            Self::MeetingBrief => "meeting_brief",
        }
    }

    /// The tier every call in this use case runs at. The tier names the
    /// `ASTRA_LLM_*` list the call's chain comes from, before fallback.
    /// The judge family rides nano (classification-sized) and economy
    /// (rule-match and the eval judges); the agent tool loop, the style
    /// inference, the knowledge extraction, the snippet search, and the
    /// meeting brief ride economy with them; autocomplete rides chat
    /// beside the assistant loop; none of those ride draft.
    pub fn tier(&self) -> ModelTier {
        match self {
            Self::AssistantChat | Self::Autocomplete => ModelTier::Chat,
            Self::DraftReply => ModelTier::Draft,
            Self::Summarise
            | Self::CategorizeSender
            | Self::CleanInbox
            | Self::PromptToRules
            | Self::Translate
            | Self::Digest
            | Self::Report => ModelTier::Default,
            Self::JudgeColdEmail
            | Self::JudgeDlp
            | Self::JudgeInjection
            | Self::JudgeDraftSurface => ModelTier::Nano,
            Self::AgentTool
            | Self::JudgeRuleMatch
            | Self::JudgeSummaryFaithful
            | Self::JudgeScenario
            | Self::StyleInfer
            | Self::KnowledgeExtract
            | Self::SnippetSearch
            | Self::MeetingBrief => ModelTier::Economy,
        }
    }
}

impl std::fmt::Display for LlmUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
