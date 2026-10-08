// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the hook API v1 wire contracts: the exact JSON every request and
//! response crosses the plugin boundary as, and the hook-to-permission mapping the
//! manifest coherence rule is built on. The wire shape is the versioned contract;
//! once plugins ship out of process it cannot change under the same API level.

use super::*;

use serde_json::json;

#[test]
fn hooks_carry_their_documented_wire_names() {
    assert_eq!(HookApiV1::ComposeTransform.wire_name(), "compose.transform");
    assert_eq!(HookApiV1::MessageDecode.wire_name(), "message.decode");
}

#[test]
fn each_hook_requires_exactly_its_permission() {
    assert_eq!(
        HookApiV1::ComposeTransform.required_permission(),
        Permission::TransformOutgoing
    );
    assert_eq!(
        HookApiV1::MessageDecode.required_permission(),
        Permission::DecodeIncoming
    );
}

#[test]
fn hooks_round_trip_through_their_envelope_names() {
    for (hook, name) in [
        (HookApiV1::ComposeTransform, "compose.transform"),
        (HookApiV1::MessageDecode, "message.decode"),
    ] {
        assert_eq!(serde_json::to_value(hook).expect("serialises"), json!(name));
        let parsed: HookApiV1 = serde_json::from_value(json!(name)).expect("parses");
        assert_eq!(parsed, hook);
    }
}

#[test]
fn the_envelope_names_match_the_wire_names() {
    // The dotted name a manifest declares, the serde envelope, and wire_name()
    // are one string, so error messages cannot disagree with the manifest an
    // author wrote.
    for hook in HookApiV1::ALL {
        let envelope = serde_json::to_value(hook).expect("serialises");
        assert_eq!(envelope, json!(hook.wire_name()));
    }
}

#[test]
fn a_compose_transform_request_serialises_its_exact_wire_shape() {
    let request = ComposeTransformRequest {
        composer_html: "<p>Hello</p>".to_owned(),
        composer_plain: Some("Hello".to_owned()),
        metadata: HookMetadata {
            account: Some(AccountRef::new("acct-1".to_owned())),
            message_id: None,
        },
    };
    assert_eq!(
        serde_json::to_value(&request).expect("serialises"),
        json!({
            "composer_html": "<p>Hello</p>",
            "composer_plain": "Hello",
            "metadata": { "account": "acct-1" },
        })
    );
    let parsed: ComposeTransformRequest =
        serde_json::from_value(serde_json::to_value(&request).expect("serialises"))
            .expect("round trips");
    assert_eq!(parsed, request);
}

#[test]
fn a_compose_request_without_a_plain_part_omits_it_on_the_wire() {
    let request = ComposeTransformRequest {
        composer_html: "<p>Hello</p>".to_owned(),
        composer_plain: None,
        metadata: HookMetadata::default(),
    };
    let wire = serde_json::to_string(&request).expect("serialises");
    assert_eq!(wire, r#"{"composer_html":"<p>Hello</p>","metadata":{}}"#);
    let parsed: ComposeTransformRequest = serde_json::from_str(&wire).expect("parses back");
    assert_eq!(parsed, request);
}

#[test]
fn a_compose_transform_response_round_trips_with_an_optional_plain_part() {
    let with_plain = ComposeTransformResponse {
        html: "<p>Signed</p>".to_owned(),
        plain: Some("Signed".to_owned()),
    };
    assert_eq!(
        serde_json::to_value(&with_plain).expect("serialises"),
        json!({ "html": "<p>Signed</p>", "plain": "Signed" })
    );
    let without_plain = ComposeTransformResponse {
        html: "<p>Signed</p>".to_owned(),
        plain: None,
    };
    assert_eq!(
        serde_json::to_value(&without_plain).expect("serialises"),
        json!({ "html": "<p>Signed</p>" })
    );
}

#[test]
fn a_message_decode_request_serialises_its_exact_wire_shape() {
    let request = MessageDecodeRequest {
        raw_message: "From: you@example.net\r\n\r\nHello".to_owned(),
        metadata: HookMetadata {
            account: None,
            message_id: Some("<msg-1@example.net>".to_owned()),
        },
    };
    assert_eq!(
        serde_json::to_value(&request).expect("serialises"),
        json!({
            "raw_message": "From: you@example.net\r\n\r\nHello",
            "metadata": { "message_id": "<msg-1@example.net>" },
        })
    );
    let parsed: MessageDecodeRequest =
        serde_json::from_value(serde_json::to_value(&request).expect("serialises"))
            .expect("round trips");
    assert_eq!(parsed, request);
}

#[test]
fn a_message_decode_response_carries_parts_and_a_banner() {
    let response = MessageDecodeResponse {
        decoded_parts: vec![DecodedPart {
            mime_type: "text/plain".to_owned(),
            content: "Hello".to_owned(),
        }],
        security_banner: Some(SecurityBanner {
            level: SecurityLevel::Warning,
            text: "Signature invalid".to_owned(),
        }),
    };
    assert_eq!(
        serde_json::to_value(&response).expect("serialises"),
        json!({
            "decoded_parts": [ { "mime_type": "text/plain", "content": "Hello" } ],
            "security_banner": { "level": "warning", "text": "Signature invalid" },
        })
    );
    let parsed: MessageDecodeResponse =
        serde_json::from_value(serde_json::to_value(&response).expect("serialises"))
            .expect("round trips");
    assert_eq!(parsed, response);
}

#[test]
fn a_decode_response_without_a_banner_omits_it_on_the_wire() {
    let response = MessageDecodeResponse {
        decoded_parts: Vec::new(),
        security_banner: None,
    };
    let wire = serde_json::to_string(&response).expect("serialises");
    assert_eq!(wire, r#"{"decoded_parts":[]}"#);
}

#[test]
fn banner_levels_round_trip_through_their_snake_case_names() {
    for (level, name) in [
        (SecurityLevel::Info, "info"),
        (SecurityLevel::Warning, "warning"),
        (SecurityLevel::Error, "error"),
    ] {
        assert_eq!(
            serde_json::to_value(level).expect("serialises"),
            json!(name)
        );
        let parsed: SecurityLevel = serde_json::from_value(json!(name)).expect("parses");
        assert_eq!(parsed, level);
    }
}
