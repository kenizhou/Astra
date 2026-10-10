// SPDX-License-Identifier: GPL-3.0-only
//! The background LLM client: one completion call resolves the use case's
//! tier chain, walks it with dual-protocol wire requests, and lands one
//! ledger row per HTTP attempt.

mod client;
mod wire;

pub use client::{LlmClient, LlmEndpoint, LlmError};
pub use wire::repair_json;

#[cfg(test)]
#[path = "fixture.rs"]
mod fixture;
