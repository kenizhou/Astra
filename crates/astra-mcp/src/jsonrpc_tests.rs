// SPDX-License-Identifier: GPL-3.0-only
//! Wire-format tests for the JSON-RPC envelope: what parses, what is
//! refused, and that a frame carries exactly one of result and error.

use serde_json::json;

use crate::server::jsonrpc::{self, Request, Response, INVALID_REQUEST, PARSE_ERROR};

#[test]
fn a_well_formed_request_parses_with_its_id_and_params() {
    let request = jsonrpc::parse(
        r#"{"jsonrpc":"2.0","id":41,"method":"tools/call","params":{"name":"ping"}}"#,
    )
    .expect("the request parses");
    assert_eq!(request.method, "tools/call");
    assert_eq!(request.id, Some(json!(41)));
    assert!(!request.is_notification());
}

#[test]
fn a_request_without_an_id_is_a_notification() {
    let request = jsonrpc::parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .expect("the notification parses");
    assert!(request.is_notification());
}

#[test]
fn malformed_json_answers_the_parse_error() {
    let error = jsonrpc::parse("{ not json").expect_err("the garbage is refused");
    assert_eq!(error.code, PARSE_ERROR);
}

#[test]
fn the_wrong_protocol_tag_answers_the_invalid_request_error() {
    let error = jsonrpc::parse(r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#).expect_err("refused");
    assert_eq!(error.code, INVALID_REQUEST);
}

#[test]
fn a_frame_carries_exactly_one_of_result_and_error() {
    let ok = Response::ok(json!(1), json!({"a": 1})).frame();
    assert!(ok.ends_with('\n'));
    let parsed: serde_json::Value = serde_json::from_str(ok.trim()).unwrap();
    assert!(parsed.get("result").is_some());
    assert!(parsed.get("error").is_none());

    let err = Response::err(
        json!(null),
        jsonrpc::RpcError::new(jsonrpc::INTERNAL_ERROR, "x"),
    )
    .frame();
    let parsed: serde_json::Value = serde_json::from_str(err.trim()).unwrap();
    assert!(parsed.get("result").is_none());
    assert!(parsed.get("error").is_some());
}

#[test]
fn a_request_round_trips_through_the_wire_shape() {
    let line = r#"{"jsonrpc":"2.0","id":"s-1","method":"ping"}"#;
    let request: Request = serde_json::from_str(line).unwrap();
    assert_eq!(request.id, Some(json!("s-1")), "a string id is legal");
}
