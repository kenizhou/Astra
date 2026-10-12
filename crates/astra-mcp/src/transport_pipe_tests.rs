// SPDX-License-Identifier: GPL-3.0-only
//! Transport tests over the real named pipe: the round trip a same-user
//! client gets, the loud failure a second bind of a taken name earns, the
//! endpoint closing when the listener stops, and the malformed-id e2e pin
//! this task owns (a tools/call whose account id cannot parse answers the
//! refusal envelope and leaks nothing, over the real transport).
//!
//! The peer check's honest coverage: every connection these tests make
//! comes from this process, which runs as the same OS user as the
//! listener, so what they prove is that a same-user peer is served. A
//! cross-user or remote-client refusal is enforced by the operating system
//! (the pipe's inherited DACL and the no-remote-clients flag) and needs a
//! second account or a second machine, which this harness has neither of;
//! the shape is recorded in the transport module's docs and the design's
//! known gaps.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use astra_ai::AiStore;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

use crate::ledger::ToolLedger;
use crate::server::Server;
use crate::tests_fake::{FakeBackend, Recorder};
use crate::transport::{Endpoint, Listener};

/// `ERROR_PIPE_BUSY`: every instance is connected and the replacement has
/// not landed yet; a retry rides it out.
const ERROR_PIPE_BUSY: i32 = 231;

/// A server over the fake backend and an in-memory ledger, allowing `ids`.
fn pipe_server(ids: &[&str]) -> Arc<Server> {
    pipe_server_with_recorder(ids).0
}

/// The server plus the recorder its writes and drafts land in.
fn pipe_server_with_recorder(ids: &[&str]) -> (Arc<Server>, Arc<Mutex<Recorder>>) {
    let (backend, recorder) = FakeBackend::new();
    let store = Arc::new(AiStore::open_in_memory().expect("the ledger opens"));
    let server = Arc::new(Server::new(
        backend,
        Arc::clone(&store) as Arc<dyn ToolLedger>,
    ));
    server
        .gate()
        .set_allowed(ids.iter().map(|id| (*id).to_owned()));
    (server, recorder)
}

/// A pipe name unique to this test process.
fn pipe(name: &str) -> String {
    format!(r"\\.\pipe\astra-mcp-test-{name}-{}", std::process::id())
}

/// Binds a listener on `name` and spawns its accept loop over `server`.
fn listening(name: &str, server: Arc<Server>) -> tokio::task::JoinHandle<()> {
    let endpoint = Endpoint::parse(&pipe(name)).expect("the test endpoint parses");
    let listener = Listener::bind(&endpoint).expect("the listener binds");
    tokio::spawn(async move {
        listener.accept(server).await;
    })
}

/// Connects a client, riding out the busy window a replacing instance
/// leaves.
async fn connect(name: &str) -> NamedPipeClient {
    for _ in 0..200 {
        match ClientOptions::new().open(pipe(name)) {
            Ok(client) => return client,
            Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Err(err) => panic!("the client connects: {err}"),
        }
    }
    panic!("the pipe stays busy past every retry");
}

/// One connected client with its line reader.
struct Client {
    writer: tokio::io::WriteHalf<NamedPipeClient>,
    reader: BufReader<tokio::io::ReadHalf<NamedPipeClient>>,
}

/// Connects and wraps the stream in the frame reader and writer.
async fn client(name: &str) -> Client {
    let stream = connect(name).await;
    let (reader, writer) = tokio::io::split(stream);
    Client {
        writer,
        reader: BufReader::new(reader),
    }
}

/// Sends one request and reads one response frame, parsed.
async fn round_trip(client: &mut Client, request: &Value) -> Value {
    client
        .writer
        .write_all(format!("{request}\n").as_bytes())
        .await
        .expect("the request writes to the pipe");
    client.writer.flush().await.expect("the request flushes");
    read_frame(client).await
}

/// Reads one newline-terminated response frame from the pipe.
async fn read_frame(client: &mut Client) -> Value {
    let mut buffer = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        let read = client
            .reader
            .read(&mut byte)
            .await
            .expect("the frame reads");
        assert_eq!(read, 1, "the pipe stays open for the answer");
        if byte[0] == b'\n' {
            break;
        }
        buffer.push(byte[0]);
    }
    serde_json::from_slice(&buffer).expect("the frame is one JSON value")
}

/// The initialize request every handshake opens with.
fn initialize() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"protocolVersion": crate::server::PROTOCOL_VERSION},
    })
}

/// A same-user client round trips the handshake, the listing, and one call
/// over the real pipe. This is the peer check's positive half: the test
/// process is the same OS user as the listener, which is the only peer the
/// pipe's security descriptor admits.
#[tokio::test]
async fn a_same_user_client_round_trips_over_the_pipe() {
    let server = pipe_server(&["work"]);
    let _listener = listening("round-trip", Arc::clone(&server));
    let mut client = client("round-trip").await;

    let hello = round_trip(&mut client, &initialize()).await;
    assert_eq!(
        hello["result"]["serverInfo"]["name"], "astra-mail",
        "{hello}"
    );

    let tools = round_trip(
        &mut client,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("the tools are a list")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a tool name"))
        .collect();
    assert_eq!(names.len(), 10, "{names:?}");
    assert!(names.contains(&"list_accounts"), "{names:?}");

    let accounts = round_trip(
        &mut client,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "list_accounts", "arguments": {}},
        }),
    )
    .await;
    // The gate allows one account of the two the fake backend holds; the
    // second is not even named.
    assert_eq!(
        accounts["result"]["structuredContent"]["accounts"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "{accounts}",
    );
}

/// A second bind of a name another listener owns fails loudly rather than
/// stealing its clients: the Windows half of never-steal-the-endpoint.
#[tokio::test]
async fn a_second_bind_of_a_taken_pipe_name_fails() {
    let endpoint = Endpoint::parse(&pipe("taken")).expect("the test endpoint parses");
    let holder = Listener::bind(&endpoint).expect("the first bind owns the name");
    assert!(
        Listener::bind(&endpoint).is_err(),
        "a second bind must not quietly take the name"
    );
    drop(holder);
}

/// Stopping the listener closes the endpoint: a client that dials after
/// the stop is refused, not left hanging.
#[tokio::test]
async fn stopping_the_listener_closes_the_endpoint() {
    let server = pipe_server(&[]);
    let listener = listening("stop", server);
    let mut client = client("stop").await;
    let hello = round_trip(&mut client, &initialize()).await;
    assert!(hello["result"].is_object(), "{hello}");

    listener.abort();
    let _ = listener.await;
    let mut refused = None;
    for _ in 0..200 {
        match ClientOptions::new().open(pipe("stop")) {
            Ok(_) => {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Err(err) => {
                refused = Some(err);
                break;
            }
        }
    }
    assert!(
        refused.is_some(),
        "the pipe must stop answering after the stop"
    );
}

/// THE malformed-id e2e pin (owned by this task): over the real transport,
/// a tools/call whose account id cannot parse answers the refusal envelope
/// and leaks nothing. The gate here holds what the settings panel would
/// never put in it, the malformed id itself, so the pin observes the
/// fail-closed below the gate rather than the gate's own refusal.
#[tokio::test]
async fn a_malformed_account_id_fails_closed_over_the_pipe() {
    let (server, recorder) = pipe_server_with_recorder(&["", "work"]);
    let _listener = listening("malformed-id", server);
    let mut client = client("malformed-id").await;
    let hello = round_trip(&mut client, &initialize()).await;
    assert!(hello["result"].is_object(), "{hello}");

    // A search naming the unparsable id answers the empty page, not the
    // unscoped search over every exposed account it could have fallen to.
    let search = round_trip(
        &mut client,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": "search_messages",
                       "arguments": {"query": "quarterly", "account": ""}},
        }),
    )
    .await;
    let page = &search["result"]["structuredContent"];
    assert_eq!(
        (
            page["rows"].as_array().map(Vec::len),
            page["total"].as_u64(),
        ),
        (Some(0), Some(0)),
        "a malformed account id answers the empty page: {search}",
    );

    // The contrast: the same search with the account omitted runs over the
    // exposed accounts and finds the fixture, so the empty page above is
    // the discipline, not an empty mailbox.
    let unscoped = round_trip(
        &mut client,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "search_messages", "arguments": {"query": "quarterly"}},
        }),
    )
    .await;
    assert_eq!(
        unscoped["result"]["structuredContent"]["total"].as_u64(),
        Some(1),
        "the unscoped search over the exposed accounts finds the fixture: {unscoped}",
    );

    // A draft from the unparsable id is refused, and nothing opened in the
    // composer on the way down.
    let draft = round_trip(
        &mut client,
        &json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {"name": "create_draft",
                       "arguments": {"account": "", "to": ["ada@example.test"],
                                     "subject": "s", "body_text": "b"}},
        }),
    )
    .await;
    assert_eq!(draft["result"]["isError"], json!(true), "{draft}");
    assert!(
        draft["result"]["content"][0]["text"]
            .as_str()
            .expect("the refusal carries a message")
            .contains("no account is configured"),
        "{draft}",
    );
    assert!(
        recorder.lock().unwrap().drafts.is_empty(),
        "nothing may open in the composer for a malformed id"
    );
}
