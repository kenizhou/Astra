// SPDX-License-Identifier: GPL-3.0-only
//! The blocked-sender field's wire crossing on the settings block: a
//! populated set crosses as a sorted array of normalised addresses, an empty
//! one crosses no key, and a pre-blocklist snapshot parses unchanged. Split
//! from the settings tests for the line cap; it reuses their block fixture.

use super::{block_fixture, SettingsBlock};

#[test]
fn the_blocked_senders_cross_when_present_and_skip_when_empty() {
    let block = SettingsBlock {
        blocked_senders: ["ada@example.test", "bob@example.test"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ..block_fixture()
    };
    let wire = serde_json::to_value(&block).expect("the block serialises");
    assert_eq!(
        wire["blocked_senders"],
        serde_json::json!(["ada@example.test", "bob@example.test"])
    );
    let back: SettingsBlock = serde_json::from_value(wire).expect("the block reads back");
    assert_eq!(back, block);

    let empty = block_fixture();
    let wire = serde_json::to_value(&empty).expect("the empty block serialises");
    assert!(
        wire.get("blocked_senders").is_none(),
        "an empty blocklist crosses no key: {wire}"
    );
    let pre_blocklist = serde_json::from_str::<SettingsBlock>(
        r#"{"theme":"light","twenty_four_clock":false,"zone":"Etc/UTC","first_day_of_week":"mo","horizon_days":42,"quote_below":true,"view_mode":"flat"}"#,
    )
    .expect("a pre-blocklist wire parses");
    assert!(
        pre_blocklist.blocked_senders.is_empty(),
        "a pre-blocklist wire reads back an empty blocklist"
    );
}
