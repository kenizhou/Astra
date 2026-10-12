// SPDX-License-Identifier: GPL-3.0-only
//! The Model Context Protocol surface for Astra Mail.
//!
//! This crate holds the contracts the MCP server and its relay build on: the
//! tool catalogue with its draft-2020-12 input schemas as literals, the
//! serde input types those schemas must agree with (so a client cannot send
//! exactly what a tool documented and get a parse error), and the
//! `<untrusted-message-content>` fence every message body is wrapped in
//! before it reaches a model's context. The design the crate serves is
//! `docs/superpowers/specs/2026-10-12-sp2k-mcp-design.md`.
//!
//! What is deliberately absent matters as much as what is present: no
//! `send_message` at all, because Astra's rulebook refuses send-class verbs
//! to the agent and humans decide and send; and no spam tool, because the
//! dispatch carries no spam verb. Both cuts are recorded in the design and
//! pinned by tests here.

pub mod fence;
pub mod tools;
