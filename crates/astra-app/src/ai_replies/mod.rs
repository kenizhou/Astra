// SPDX-License-Identifier: GPL-3.0-only
//! The AI replies model: the draft pool's app-layer shape and the reply
//! generation path that fills it, the A2 slice's store-facing half. The
//! pool row types and CRUD live in the AI foundation (migration 0004
//! beside its style profile sibling, 0005's summarise cache); this
//! module owns the context collector, the versioned prompt, the
//! structured `{reply, confidence}` parse, and the failure path that
//! records a skipped row. The surfacing gate, the cached summarise
//! action, and the style inference land in their own child modules with
//! their slices.

mod draft;

#[cfg(test)]
#[path = "draft_tests.rs"]
mod draft_tests;

pub use draft::{
    draft_prompt, generate_reply, DraftConfidence, DraftContext, DraftPoolEntry,
    DRAFT_REPLY_PROMPT_VERSION, THREAD_BODY_CAP, THREAD_HISTORY_LIMIT,
};
