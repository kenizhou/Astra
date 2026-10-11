// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the use-case registry's later rows, beside the tier table's
//! own pins: each slice's use cases mapped to their tier and their
//! snake_case wire word, both ways, so a renamed word or a moved tier can
//! never slip out silently.

use crate::{LlmUseCase, ModelTier};

#[test]
fn meeting_brief_pins_the_economy_tier_and_its_wire_word() {
    // The SP2h meeting brief is summarisation-sized work over a day's
    // meetings, the shape the economy tier exists for; the tier comes from
    // this registry row, never a literal at the call site.
    assert_eq!(LlmUseCase::MeetingBrief.tier(), ModelTier::Economy);
    assert_eq!(
        serde_json::to_string(&LlmUseCase::MeetingBrief).unwrap(),
        "\"meeting_brief\"",
    );
    assert_eq!(LlmUseCase::MeetingBrief.as_str(), "meeting_brief");
    assert_eq!(
        serde_json::from_str::<LlmUseCase>("\"meeting_brief\"").unwrap(),
        LlmUseCase::MeetingBrief,
    );
    assert_eq!(
        LlmUseCase::MeetingBrief.to_string(),
        "meeting_brief",
        "the display spelling is the wire word",
    );
}
