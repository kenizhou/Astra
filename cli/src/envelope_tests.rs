// SPDX-License-Identifier: GPL-3.0-only
//! Byte-pinned tests for the `--json` envelope: both shapes, exactly as the
//! sidecar's parser will read them.

use serde_json::json;

use super::envelope::{self, Code, Envelope};

#[test]
fn the_schema_version_is_pinned_at_one() {
    assert_eq!(envelope::SCHEMA, 1);
    assert_eq!(Envelope::success(json!(null)).schema(), envelope::SCHEMA);
}

#[test]
fn a_success_envelope_serialises_byte_for_byte() {
    let body = Envelope::success(json!({"keys": ["a"], "count": 1}));
    assert_eq!(
        body.render(false).unwrap(),
        r#"{"schema":1,"ok":true,"data":{"count":1,"keys":["a"]}}"#
    );
}

#[test]
fn an_error_envelope_serialises_byte_for_byte() {
    let body = Envelope::failure(Code::Usage, "the --account flag is required");
    assert_eq!(
        body.render(false).unwrap(),
        r#"{"schema":1,"ok":false,"error":{"code":"usage","message":"the --account flag is required"}}"#
    );
}

#[test]
fn a_success_envelope_carries_no_error_field() {
    let text = Envelope::success(json!({})).render(false).unwrap();
    assert!(text.contains(r#""ok":true"#));
    assert!(!text.contains("error"));
}

#[test]
fn an_error_envelope_carries_no_data_field() {
    let text = Envelope::failure(Code::Runtime, "no store")
        .render(false)
        .unwrap();
    assert!(text.contains(r#""ok":false"#));
    assert!(!text.contains("data"));
}

#[test]
fn every_code_names_its_exit_status() {
    assert_eq!(Code::Ok.exit(), 0);
    assert_eq!(Code::Runtime.exit(), 1);
    assert_eq!(Code::Usage.exit(), 2);
    assert_eq!(Code::Refused.exit(), 3);
}

#[test]
fn every_code_spells_its_wire_word() {
    assert_eq!(Code::Usage.wire(), "usage");
    assert_eq!(Code::Runtime.wire(), "runtime");
    assert_eq!(Code::Refused.wire(), "refused");
}

#[test]
fn pretty_rendering_still_parses_to_the_same_value() {
    let body = Envelope::success(json!({"n": 1}));
    let compact = body.render(false).unwrap();
    let pretty = body.render(true).unwrap();
    assert_ne!(compact, pretty);
    assert!(pretty.contains('\n'));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&compact).unwrap(),
        serde_json::from_str::<serde_json::Value>(&pretty).unwrap()
    );
}
