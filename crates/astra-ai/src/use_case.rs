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
    /// The agent tool loop. Inbox-zero maps its agent use cases (the
    /// `McpAgent` line) to the economy tier; `agent_tool` follows.
    AgentTool,
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
            Self::AgentTool => "agent_tool",
        }
    }

    /// The tier every call in this use case runs at. The tier names the
    /// `ASTRA_LLM_*` list the call's chain comes from, before fallback.
    pub fn tier(&self) -> ModelTier {
        match self {
            Self::AssistantChat => ModelTier::Chat,
            Self::DraftReply => ModelTier::Draft,
            Self::Summarise | Self::CategorizeSender | Self::CleanInbox => ModelTier::Default,
            Self::AgentTool => ModelTier::Economy,
        }
    }
}

impl std::fmt::Display for LlmUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
