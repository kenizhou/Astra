// SPDX-License-Identifier: GPL-3.0-only
//! The AI replies model: the draft pool's app-layer shape and the reply
//! generation path that fills it, the surfacing gate and the chips data
//! it answers, the cached summarise action, and the per-account style
//! profile, the A2 slice's store-facing half. The pool row types and CRUD
//! live in the AI foundation (migration 0004 beside its style profile
//! sibling, 0005's summarise cache, 0006's chips-dismissed markers); this
//! module owns the context collector, the versioned prompts, the
//! structured parses, and the failure paths that record skipped rows or
//! fall back to stored state. Its `drive` child owns the runtime half:
//! the arming an open leaves, the driver the host runs, and the host's
//! AI pulls over the one bundle.

mod draft;
mod drive;
mod gate;
mod style;
mod summarise;

#[cfg(test)]
#[path = "draft_tests.rs"]
mod draft_tests;

#[cfg(test)]
#[path = "gate_tests.rs"]
mod gate_tests;

#[cfg(test)]
#[path = "summarise_tests.rs"]
mod summarise_tests;

#[cfg(test)]
#[path = "style_tests.rs"]
mod style_tests;

pub use draft::{
    draft_prompt, generate_reply, DraftConfidence, DraftContext, DraftPoolEntry,
    DRAFT_REPLY_PROMPT_VERSION, THREAD_BODY_CAP, THREAD_HISTORY_LIMIT,
};
pub(crate) use drive::DraftArm;
pub use drive::{DraftPoolSink, SummariseMessageError, STYLE_SENT_WINDOW};
pub use gate::{
    chips_for, surface_question, Chip, CHIP_CONFIDENCE_FLOOR, CHIP_LIMIT, CHIP_PREVIEW_CAP,
    GATE_JUDGE_VERSION,
};
pub use style::{
    style_profile, style_prompt, STYLE_PROFILE_MAX_AGE_SECS, STYLE_PROMPT_VERSION,
    STYLE_SAMPLE_BODY_CAP, STYLE_SAMPLE_LIMIT,
};
pub use summarise::{
    body_hash, summarise, summarise_prompt, SummariseError, SUMMARISE_PROMPT_VERSION,
};
