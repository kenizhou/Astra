// SPDX-License-Identifier: GPL-3.0-only
//! The background LLM client: one completion call resolves the use case's
//! tier chain, walks it with dual-protocol wire requests, and lands one
//! ledger row per HTTP attempt.

mod client;
mod seams;
mod wire;

pub use astra_ai::repair_json;
pub use client::{LlmClient, LlmEndpoint, LlmError};
pub(crate) use seams::DraftPoolEventSink;
pub use seams::{incoming_pass, LlmAiComplete, LlmAiJudge};

#[cfg(test)]
#[path = "live_tests.rs"]
mod live_tests;

#[cfg(test)]
#[path = "live_assist_tests.rs"]
mod live_assist_tests;

#[cfg(test)]
#[path = "live_replies_tests.rs"]
mod live_replies_tests;

#[cfg(test)]
#[path = "fixture.rs"]
mod fixture;
