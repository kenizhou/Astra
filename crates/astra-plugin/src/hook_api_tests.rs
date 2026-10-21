// SPDX-License-Identifier: GPL-3.0-only
//! Wire pins for the hook API v1 payload types: the request/response schemas
//! the `composer.transform` and `message.decode` object methods carry. These
//! pins are the breaking-change alarm for SP3b's wiring: a shape that moves
//! without a version bump fails here first.

use super::*;

use serde_json::json;

#[test]
fn the_compose_transform_request_pins_its_wire_shape() {
    let request = ComposeTransformRequest {
        composer_html: "<p>hello</p>".to_string(),
        composer_plain: Some("hello".to_string()),
        metadata: HookMetadata {
            account: Some(AccountRef::new("work".to_string())),
            message_id: None,
        },
    };
    assert_eq!(
        serde_json::to_value(&request).expect("serialises"),
        json!({
            "composer_html": "<p>hello</p>",
            "composer_plain": "hello",
            "metadata": { "account": "work" },
        })
    );
}

#[test]
fn the_compose_transform_response_pins_its_wire_shape() {
    let response = ComposeTransformResponse {
        html: "<p>signed</p>".to_string(),
        plain: None,
    };
    assert_eq!(
        serde_json::to_value(&response).expect("serialises"),
        json!({ "html": "<p>signed</p>" })
    );
}

#[test]
fn the_message_decode_request_pins_its_wire_shape() {
    let request = MessageDecodeRequest {
        raw_message: "From: a@b.test\r\n\r\nhi".to_string(),
        metadata: HookMetadata {
            account: None,
            message_id: Some("<m1@b.test>".to_string()),
        },
    };
    assert_eq!(
        serde_json::to_value(&request).expect("serialises"),
        json!({
            "raw_message": "From: a@b.test\r\n\r\nhi",
            "metadata": { "message_id": "<m1@b.test>" },
        })
    );
}

#[test]
fn the_message_decode_response_pins_its_wire_shape() {
    let response = MessageDecodeResponse {
        decoded_parts: vec![DecodedPart {
            mime_type: "text/plain".to_string(),
            content: "hi".to_string(),
        }],
        security_banner: Some(SecurityBanner {
            level: SecurityLevel::Warning,
            text: "Signature invalid".to_string(),
        }),
    };
    assert_eq!(
        serde_json::to_value(&response).expect("serialises"),
        json!({
            "decoded_parts": [{ "mime_type": "text/plain", "content": "hi" }],
            "security_banner": { "level": "warning", "text": "Signature invalid" },
        })
    );
}

#[test]
fn absent_options_are_omitted_not_nulled() {
    let metadata = HookMetadata::default();
    assert_eq!(
        serde_json::to_value(&metadata).expect("serialises"),
        json!({})
    );
}
