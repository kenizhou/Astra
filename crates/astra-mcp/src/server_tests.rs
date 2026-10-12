// SPDX-License-Identifier: GPL-3.0-only
//! Server tests: the JSON-RPC framing round trips and the standard error
//! envelopes, the gate over the wire (empty answers nothing; an unlisted
//! account refused; the gate read fresh on every call), and the ledger
//! discipline (a row per call with the right shape, a refused ledger fails
//! the call closed, the character count and never the content). The tests
//! drive the real connection loop over an in-memory duplex stream.

use std::sync::Arc;

use astra_ai::AiStore;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};

use crate::ledger::{LedgerClosed, Outcome, ToolCallRecord, ToolLedger};
use crate::server::Server;
use crate::tests_fake::FakeBackend;

/// A ledger that refuses every row, for the fail-closed pin.
struct ClosedLedger;

impl ToolLedger for ClosedLedger {
    fn record(&self, _record: &ToolCallRecord) -> Result<(), LedgerClosed> {
        Err(LedgerClosed::new("closed for the test".to_owned()))
    }
}

/// A server over the fake backend and an in-memory ledger, allowing the
/// accounts named.
fn server(ids: &[&str]) -> (Arc<Server>, Arc<AiStore>) {
    let (backend, _) = FakeBackend::new();
    let store = Arc::new(AiStore::open_in_memory().expect("the ledger opens"));
    let server = Arc::new(Server::new(
        backend,
        Arc::clone(&store) as Arc<dyn ToolLedger>,
    ));
    server
        .gate()
        .set_allowed(ids.iter().map(|id| (*id).to_owned()));
    (server, store)
}

/// A server over the fake backend and the closed ledger.
fn closed_server() -> Arc<Server> {
    let (backend, _) = FakeBackend::new();
    Arc::new(Server::new(backend, Arc::new(ClosedLedger)))
}

/// One connected client half, with its line reader beside it.
struct Client {
    writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    reader: BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
}

/// Spawns the server on one end of a duplex stream and hands over the
/// other.
fn connect(server: &Arc<Server>) -> Client {
    let (client, server_end) = tokio::io::duplex(64 * 1024);
    let server = Arc::clone(server);
    tokio::spawn(async move {
        server.serve_connection(server_end).await;
    });
    let (reader, writer) = tokio::io::split(client);
    Client {
        writer,
        reader: BufReader::new(reader),
    }
}

/// Sends one request line and reads one response frame, parsed.
async fn round_trip(client: &mut Client, request: &Value) -> Value {
    client
        .writer
        .write_all(format!("{request}\n").as_bytes())
        .await
        .expect("the request writes");
    client.writer.flush().await.expect("the request flushes");
    read_frame(client).await
}

/// Sends one raw line and reads one response frame, parsed.
async fn round_trip_raw(client: &mut Client, line: &str) -> Value {
    client
        .writer
        .write_all(format!("{line}\n").as_bytes())
        .await
        .expect("the raw line writes");
    client.writer.flush().await.expect("the line flushes");
    read_frame(client).await
}

/// Reads one newline-terminated response frame.
async fn read_frame(client: &mut Client) -> Value {
    let mut buffer = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        let read = client
            .reader
            .read(&mut byte)
            .await
            .expect("the frame reads");
        assert_eq!(read, 1, "the connection stays open for the answer");
        if byte[0] == b'\n' {
            break;
        }
        buffer.push(byte[0]);
    }
    serde_json::from_slice(&buffer).expect("the frame is one JSON value")
}

/// The initialize request every handshake test opens with.
fn initialize() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"protocolVersion": crate::server::PROTOCOL_VERSION},
    })
}

/// A tools/call request for `tool` with `arguments`.
fn tool_call(id: i64, tool: &str, arguments: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": tool, "arguments": arguments},
    })
}

#[tokio::test]
async fn initialize_round_trips_the_handshake() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    let answer = round_trip(&mut client, &initialize()).await;
    assert_eq!(answer["jsonrpc"], "2.0");
    assert_eq!(answer["id"], 1, "the id echoes verbatim");
    assert_eq!(
        answer["result"]["protocolVersion"],
        crate::server::PROTOCOL_VERSION
    );
    assert_eq!(
        answer["result"]["capabilities"],
        json!({"tools": {"listChanged": false}}),
    );
    assert_eq!(answer["result"]["serverInfo"]["name"], "astra-mail");
    assert_eq!(answer["result"]["serverInfo"]["title"], "Astra Mail");
}

#[tokio::test]
async fn initialize_counter_offers_its_own_version_on_an_unknown_one() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    let answer = round_trip(
        &mut client,
        &json!({
            "jsonrpc": "2.0", "id": 7, "method": "initialize",
            "params": {"protocolVersion": "1999-01-01"},
        }),
    )
    .await;
    assert_eq!(
        answer["result"]["protocolVersion"],
        crate::server::PROTOCOL_VERSION,
        "an unknown version is counter-offered, never echoed"
    );
}

#[tokio::test]
async fn tools_list_answers_the_catalogue_with_schemas_and_annotations() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    let answer = round_trip(
        &mut client,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    let tools = answer["result"]["tools"]
        .as_array()
        .expect("the tools list");
    assert_eq!(tools.len(), 10, "the ten tools of the catalogue");
    for tool in tools {
        assert!(tool["inputSchema"].is_object(), "each entry has a schema");
        assert!(tool["annotations"].is_object(), "each entry is annotated");
    }
    assert_eq!(tools[0]["name"], "list_accounts");
    assert_eq!(tools[9]["name"], "create_draft");
}

#[tokio::test]
async fn a_tools_call_round_trips_over_the_wire() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    let answer = round_trip(&mut client, &tool_call(3, "list_accounts", &json!({}))).await;
    assert_eq!(answer["id"], 3);
    assert_eq!(answer["result"]["isError"], false);
    assert_eq!(
        answer["result"]["structuredContent"],
        json!({"accounts": [{"id": "work", "address": "me@work.example"}]}),
    );
    assert!(
        answer["result"]["content"][0]["text"].is_string(),
        "the text fallback carries the same answer"
    );
}

#[tokio::test]
async fn a_notification_is_never_answered() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    // Both lines in one write: the only answer that may come back is the
    // ping's, proving the notification itself drew none.
    client
        .writer
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n\
              {\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n",
        )
        .await
        .expect("the lines write");
    client.writer.flush().await.expect("the lines flush");
    let answer = read_frame(&mut client).await;
    assert_eq!(answer["id"], 9, "the ping's answer, and nothing before it");
    assert_eq!(answer["result"], json!({}));
}

#[tokio::test]
async fn a_garbage_line_answers_the_parse_error_envelope() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    let answer = round_trip_raw(&mut client, "not json at all").await;
    assert_eq!(answer["id"], Value::Null);
    assert_eq!(answer["error"]["code"], -32_700);
}

#[tokio::test]
async fn a_wrong_protocol_version_answers_the_invalid_request_envelope() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    let answer = round_trip_raw(
        &mut client,
        "{\"jsonrpc\":\"1.0\",\"id\":1,\"method\":\"ping\"}",
    )
    .await;
    assert_eq!(answer["error"]["code"], -32_600);
}

#[tokio::test]
async fn an_unknown_method_answers_the_method_not_found_envelope() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    let answer = round_trip(
        &mut client,
        &json!({"jsonrpc": "2.0", "id": 4, "method": "resources/list"}),
    )
    .await;
    assert_eq!(answer["error"]["code"], -32_601);
    assert!(
        answer["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("resources/list")),
        "the envelope names the unknown method"
    );
}

#[tokio::test]
async fn a_call_without_a_tool_name_answers_the_invalid_params_envelope() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    let answer = round_trip(
        &mut client,
        &json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {}}),
    )
    .await;
    assert_eq!(answer["error"]["code"], -32_602);
}

#[tokio::test]
async fn an_empty_gate_answers_nothing_over_the_wire() {
    let (server, _) = server(&[]);
    let mut client = connect(&server);
    let answer = round_trip(&mut client, &tool_call(1, "list_accounts", &json!({}))).await;
    let structured = answer["result"]["structuredContent"].to_string();
    assert_eq!(
        structured, "{\"accounts\":[]}",
        "empty answers an empty list, not even a count"
    );
    let frame = answer.to_string();
    assert!(
        !frame.contains("work") && !frame.contains("private") && !frame.contains("me@"),
        "no account is named, not even by accident: {frame}"
    );
}

#[tokio::test]
async fn an_unlisted_account_is_refused_as_a_tool_error_over_the_wire() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    let answer = round_trip(
        &mut client,
        &tool_call(
            2,
            "get_message",
            &json!({"account": "private", "key": "m1"}),
        ),
    )
    .await;
    assert_eq!(
        answer["result"]["isError"], true,
        "a gate refusal is a tool error the model sees, not a protocol error"
    );
    let message = answer["result"]["content"][0]["text"].as_str().unwrap();
    assert!(message.contains("is not available to assistants"));
}

#[tokio::test]
async fn the_gate_is_read_fresh_on_every_call() {
    let (server, _) = server(&["work"]);
    let mut client = connect(&server);
    let allowed = round_trip(&mut client, &tool_call(1, "list_accounts", &json!({}))).await;
    assert_eq!(
        allowed["result"]["structuredContent"]["accounts"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "one account allowed"
    );

    server.gate().set_allowed(Vec::<String>::new());
    let cleared = round_trip(&mut client, &tool_call(2, "list_accounts", &json!({}))).await;
    assert_eq!(
        cleared["result"]["structuredContent"].to_string(),
        "{\"accounts\":[]}",
        "revoking applies to the next call on a live connection"
    );
}

#[tokio::test]
async fn every_call_lands_one_row_with_the_right_shape() {
    let (server, store) = server(&["work"]);
    let mut client = connect(&server);
    round_trip(&mut client, &tool_call(1, "list_accounts", &json!({}))).await;
    round_trip(
        &mut client,
        &tool_call(
            2,
            "mark_read",
            &json!({"account": "work", "key": "m1", "read": true}),
        ),
    )
    .await;

    let rows = store.recent_tool_calls(10).expect("the ledger reads");
    assert_eq!(rows.len(), 2, "one row per call, the refusal included");
    let newest_first: Vec<&str> = rows.iter().map(|row| row.tool.as_str()).collect();
    assert_eq!(newest_first, ["mark_read", "list_accounts"]);
    assert_eq!(rows[0].outcome, "ok");
    assert_eq!(rows[1].outcome, "ok");
    assert!(
        rows[0].correlation != rows[1].correlation,
        "the correlation id is generated per call"
    );
}

#[tokio::test]
async fn a_refused_call_lands_a_row_too() {
    let (server, store) = server(&["work"]);
    let mut client = connect(&server);
    round_trip(
        &mut client,
        &tool_call(
            1,
            "get_message",
            &json!({"account": "private", "key": "m1"}),
        ),
    )
    .await;
    let rows = store.recent_tool_calls(10).expect("the ledger reads");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tool, "get_message");
    assert_eq!(rows[0].outcome, "refused");
}

#[tokio::test]
async fn a_ledger_that_cannot_record_fails_the_call_closed() {
    let server = closed_server();
    let mut client = connect(&server);
    let answer = round_trip(&mut client, &tool_call(1, "list_accounts", &json!({}))).await;
    assert_eq!(
        answer["error"]["code"], -32_603,
        "a call that cannot be recorded does not pass unrecorded"
    );
    assert!(
        answer["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("ledger")),
        "the envelope says what failed: {answer}"
    );
}

/// The never-content pin end to end: the row's arg_chars equals the query's
/// own character count (characters, not bytes, so the multi-byte query
/// pins the stronger property), and the row holds nowhere the query itself.
#[tokio::test]
async fn the_ledger_counts_the_query_and_never_holds_it() {
    let (server, store) = server(&["work"]);
    let query = "café ☕ secret-needle";
    let mut client = connect(&server);
    round_trip(
        &mut client,
        &tool_call(1, "search_messages", &json!({"query": query})),
    )
    .await;

    let rows = store.recent_tool_calls(10).expect("the ledger reads");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].arg_chars,
        query.chars().count() as u64,
        "the row counts the characters the caller sent"
    );
    let row = format!("{:?}", rows[0]);
    assert!(
        !row.contains("secret-needle") && !row.contains("café"),
        "the row never holds the query: {row}"
    );
}

#[tokio::test]
async fn the_outcome_vocabulary_is_spelled_in_full() {
    assert_eq!(Outcome::Ok.as_str(), "ok");
    assert_eq!(Outcome::Refused.as_str(), "refused");
    assert_eq!(Outcome::InvalidParams.as_str(), "invalid_params");
    assert_eq!(Outcome::UnknownTool.as_str(), "unknown_tool");
    assert_eq!(Outcome::Internal.as_str(), "internal");
}
