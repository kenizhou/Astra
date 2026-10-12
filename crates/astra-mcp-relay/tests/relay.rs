// SPDX-License-Identifier: GPL-3.0-only
//! Relay tests over the real transport: the built binary, a real named
//! pipe, real stdin and stdout.
//!
//! The byte-count pin is the load-bearing one: the relay's whole contract
//! is that it knows nothing, so N bytes in must produce exactly N bytes
//! out, binary garbage included, newlines mid-stream included, and a
//! stream with no trailing newline included. Anything else on stdout, or
//! any transformation at all, changes the count and fails here.
//!
//! The not-running case pins this relay's recorded deviation from
//! mail-calendar's shim: the shim answers an unreachable app with a
//! JSON-RPC error frame, which means reading the request's id, which is
//! protocol knowledge this relay refuses. So the relay exits non-zero
//! with the diagnostic on stderr and stdout untouched, and the client
//! reports a failed server, which the user resolves by opening the app.

#![cfg(windows)]

use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::ServerOptions;
use tokio::process::{Child, Command};

use astra_app::{MailWriteError, MessageDetail, MessagePage, OpenDraftError, QueryAccount};
use astra_mcp::backend::{AgentDraft, MailBackend};
use astra_mcp::ledger::ToolLedger;
use astra_mcp::server::Server;
use astra_mcp::transport::{Endpoint, Listener};
use astra_viewmodel::FolderRow;
use engine_api::AccountId;

/// The built relay binary, as cargo hands its path to the test.
const BIN: &str = env!("CARGO_BIN_EXE_astra-mcp-relay");

/// No reply is worth waiting ten seconds for; a wedged relay must fail
/// the test, not hang it.
const BOUND: Duration = Duration::from_secs(10);

/// Long enough for the relay to be parked reading the pipe when the next
/// write comes: two lines written back-to-back beat the reader to the
/// handle and sail through, so without this beat a relay that serializes
/// its two directions passes anyway.
const SETTLE: Duration = Duration::from_millis(250);

/// A pipe name unique to one test, so no sibling test's endpoint answers.
fn pipe_name(name: &str) -> String {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_nanos();
    format!(r"\\.\pipe\astra-relay-test-{name}-{unique}")
}

/// Spawns the relay against `endpoint`, with piped stdio and no chance of
/// outliving a failing test.
fn spawn_relay(endpoint: &str) -> Child {
    Command::new(BIN)
        .arg("--endpoint")
        .arg(endpoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the relay binary runs")
}

/// Reads exactly `n` bytes, failing the test on the bound rather than
/// hanging on a wedged relay.
async fn read_exact_bounded<R: AsyncReadExt + Unpin>(
    reader: &mut R,
    n: usize,
    what: &str,
) -> Vec<u8> {
    let mut buffer = vec![0_u8; n];
    timeout(reader.read_exact(&mut buffer), what)
        .await
        .expect("the exact read succeeds");
    buffer
}

/// Runs `what` under the test's bound, naming the wait on failure.
async fn timeout<F>(future: F, what: &str) -> F::Output
where
    F: std::future::Future,
{
    tokio::time::timeout(BOUND, future)
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}: the relay is wedged"))
}

/// Deterministic binary garbage: every byte value over its length, so the
/// payload carries newlines mid-stream, NULs, and high bytes, and the
/// caller picks a length whose last byte is not a newline.
#[allow(clippy::cast_possible_truncation)]
// The mask makes the truncation the point: every byte value is possible,
// including the hostile ones.
fn garbage(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| ((i * 7 + usize::from(seed)) & 0xff) as u8)
        .collect()
}

/// THE pin: N bytes in produce exactly N bytes out, both directions, with
/// binary garbage and no trailing newline.
#[tokio::test]
async fn n_bytes_in_answer_exactly_n_bytes_out() {
    let endpoint = pipe_name("byte-count");
    let server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&endpoint)
        .expect("the test pipe binds");
    let mut child = spawn_relay(&endpoint);
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut stdout = child.stdout.take().expect("piped stdout");

    timeout(server.connect(), "the relay to dial the pipe")
        .await
        .expect("the relay connects");
    let (mut app_read, mut app_write) = tokio::io::split(server);

    let outbound = garbage(4097, 0);
    assert!(
        outbound.contains(&b'\n'),
        "the fixture carries mid-stream newlines"
    );
    assert_ne!(*outbound.last().expect("non-empty"), b'\n');
    stdin.write_all(&outbound).await.expect("the bytes go in");
    stdin.flush().await.expect("the write flushes");

    let relayed = read_exact_bounded(&mut app_read, outbound.len(), "the request bytes").await;
    assert_eq!(
        relayed, outbound,
        "N bytes in, exactly N bytes out, verbatim: a hostile byte is a byte",
    );

    let inbound = garbage(3001, 3);
    assert_ne!(*inbound.last().expect("non-empty"), b'\n');
    app_write
        .write_all(&inbound)
        .await
        .expect("the reply bytes go out");
    app_write.flush().await.expect("the reply flushes");
    let answered = read_exact_bounded(&mut stdout, inbound.len(), "the reply bytes").await;
    assert_eq!(answered, inbound, "the other direction is verbatim too");

    drop(stdin);
    let status = timeout(child.wait(), "the relay exits")
        .await
        .expect("exit status");
    assert!(
        status.success(),
        "stdin closing ends the relay cleanly: {status}",
    );
}

/// A backend holding one account and nothing else: enough for a round
/// trip through the real server without an engine.
struct OneAccountBackend;

#[async_trait]
impl MailBackend for OneAccountBackend {
    async fn accounts(&self) -> Vec<QueryAccount> {
        vec![QueryAccount {
            id: AccountId::try_from("work").expect("a well-formed id"),
            address: "me@work.example".to_owned(),
        }]
    }

    async fn folders(&self, _account: &str) -> Vec<FolderRow> {
        Vec::new()
    }

    async fn folder_page(
        &self,
        _account: &str,
        _folder: Option<&str>,
        _unread_only: bool,
        offset: usize,
        _limit: usize,
    ) -> MessagePage {
        MessagePage {
            rows: Vec::new(),
            total: 0,
            offset,
            windowed: false,
        }
    }

    async fn search(
        &self,
        _query: &str,
        _account: Option<&str>,
        _folder: Option<&str>,
        offset: usize,
        _limit: usize,
    ) -> MessagePage {
        MessagePage {
            rows: Vec::new(),
            total: 0,
            offset,
            windowed: false,
        }
    }

    async fn search_set(
        &self,
        _query: &str,
        _accounts: &[String],
        offset: usize,
        _limit: usize,
    ) -> MessagePage {
        MessagePage {
            rows: Vec::new(),
            total: 0,
            offset,
            windowed: false,
        }
    }

    async fn message(&self, _account: &str, _key: &str) -> Option<MessageDetail> {
        None
    }

    async fn mark_read(
        &self,
        _account: &str,
        _key: &str,
        _read: bool,
    ) -> Result<(), MailWriteError> {
        Ok(())
    }

    async fn set_flagged(
        &self,
        _account: &str,
        _key: &str,
        _flagged: bool,
    ) -> Result<(), MailWriteError> {
        Ok(())
    }

    async fn archive(&self, _account: &str, _key: &str) -> Result<(), MailWriteError> {
        Ok(())
    }

    async fn trash(&self, _account: &str, _key: &str) -> Result<(), MailWriteError> {
        Ok(())
    }

    async fn open_composer(&self, _draft: AgentDraft) -> Result<(), OpenDraftError> {
        Ok(())
    }
}

/// One line read from the relay's stdout, bounded.
async fn read_line_bounded(
    stdout: &mut BufReader<tokio::process::ChildStdout>,
) -> serde_json::Value {
    let mut line = Vec::new();
    timeout(stdout.read_until(b'\n', &mut line), "a response line")
        .await
        .expect("the line reads");
    assert_eq!(line.pop(), Some(b'\n'), "the frame ends with the newline");
    serde_json::from_slice(&line).expect("the server answers one JSON value")
}

/// The echo round trip: a client's initialize, a notification that draws
/// nothing, a second request after the first reply, and a tool call, all
/// through relay, pipe, and the real server. The settle beats hold the
/// deadlock shapes mail-calendar's Windows shim shipped with: a relay
/// whose parked read blocks its own write passes one request and hangs
/// the second, so the test makes the relay park before each next write.
#[tokio::test]
async fn a_client_round_trips_through_the_relay_to_the_server() {
    let endpoint = pipe_name("round-trip");
    let server = Arc::new(Server::new(
        Arc::new(OneAccountBackend) as Arc<dyn MailBackend>,
        Arc::new(astra_ai::AiStore::open_in_memory().expect("the ledger opens"))
            as Arc<dyn ToolLedger>,
    ));
    server.gate().set_allowed(["work".to_owned()]);
    let listener = Listener::bind(&Endpoint::parse(&endpoint).expect("the endpoint parses"))
        .expect("the listener binds");
    let accept = Arc::clone(&server);
    tokio::spawn(async move {
        listener.accept(accept).await;
    });

    let mut child = spawn_relay(&endpoint);
    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let mut stdout = BufReader::new(stdout);

    stdin
        .write_all(
            format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\
                 \"protocolVersion\":\"{}\"}}}}\n",
                astra_mcp::server::PROTOCOL_VERSION
            )
            .as_bytes(),
        )
        .await
        .expect("the handshake writes");
    stdin.flush().await.expect("the handshake flushes");
    let hello = read_line_bounded(&mut stdout).await;
    assert_eq!(
        hello["result"]["serverInfo"]["name"], "astra-mail",
        "{hello}"
    );

    // A notification draws no reply, which parks the relay on the pipe
    // with nothing coming: exactly the state that wedged a serializing
    // relay, and the next request below must still go through.
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .expect("the notification writes");
    stdin.flush().await.expect("the notification flushes");
    tokio::time::sleep(SETTLE).await;

    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n")
        .await
        .expect("the listing writes");
    stdin.flush().await.expect("the listing flushes");
    let tools = read_line_bounded(&mut stdout).await;
    assert_eq!(
        tools["result"]["tools"].as_array().map(Vec::len),
        Some(10),
        "{tools}",
    );

    stdin
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\
             \"list_accounts\",\"arguments\":{}}}\n",
        )
        .await
        .expect("the call writes");
    stdin.flush().await.expect("the call flushes");
    let accounts = read_line_bounded(&mut stdout).await;
    assert_eq!(
        accounts["result"]["structuredContent"]["accounts"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "the one exposed account is named: {accounts}",
    );

    drop(stdin);
    let status = timeout(child.wait(), "the relay exits")
        .await
        .expect("exit status");
    assert!(status.success(), "the relay ends cleanly: {status}");
}

/// The recorded deviation: the app not running ends the relay non-zero,
/// with the diagnostic on stderr and stdout untouched.
#[tokio::test]
async fn an_unreachable_app_exits_nonzero_with_stdout_untouched() {
    let endpoint = pipe_name("not-running");
    let mut child = spawn_relay(&endpoint);
    let mut stdin = child.stdin.take().expect("piped stdin");
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
        .await
        .expect("the request writes");
    drop(stdin);

    let output = timeout(child.wait_with_output(), "the relay exits")
        .await
        .expect("the relay exits");
    assert!(
        !output.status.success(),
        "the relay reports failure when it cannot reach the app",
    );
    assert!(
        output.stdout.is_empty(),
        "nothing but relayed bytes ever reach stdout, and there were none to relay",
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).is_empty(),
        "the diagnostic goes to stderr, where it belongs",
    );
}

/// No endpoint argument is a usage error: non-zero, stdout untouched.
#[tokio::test]
async fn a_missing_endpoint_argument_is_a_usage_error() {
    let output = Command::new(BIN)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .expect("the relay binary runs");
    assert!(
        !output.status.success(),
        "a missing argument fails the start"
    );
    assert!(output.stdout.is_empty(), "usage never touches stdout");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("usage"),
        "the usage note names the fix: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}
