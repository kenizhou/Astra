// SPDX-License-Identifier: GPL-3.0-only
#![cfg(windows)]
//! McpHost tests: the run/stop seam over a real pipe. Enabling binds and
//! serves; disabling closes the endpoint; a taken name lands in the
//! failed state the panel reads rather than a crash; the allowlist seam
//! is the gate the wire sees. The tests follow the crate's drive pattern:
//! futures build and run on a runtime of our own, never on the caller's
//! stack.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use astra_app::{MailWriteError, MessageDetail, MessagePage, OpenDraftError, QueryAccount};
use astra_mcp::backend::{AgentDraft, MailBackend};
use astra_mcp::ledger::ToolLedger;
use astra_mcp::server::Server;
use astra_mcp::transport::Listener;
use astra_viewmodel::FolderRow;
use engine_api::AccountId;

use super::{McpHost, McpStatus};

/// A scratch data directory whose leaf names a pipe nobody else holds.
fn data_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("astra-mcp-host-{name}-{}", std::process::id()))
}

/// A backend holding one account, so the wire has something to answer
/// with; the full port behaviour is pinned in astra-mcp's own suites.
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

/// A host over the one-account backend and an in-memory ledger, allowing
/// `ids`, with its own runtime.
fn host(name: &str, ids: &[&str]) -> (crate::runtime::AppRuntime, Arc<McpHost>) {
    let server = Arc::new(Server::new(
        Arc::new(OneAccountBackend) as Arc<dyn MailBackend>,
        Arc::new(astra_ai::AiStore::open_in_memory().expect("the ledger opens"))
            as Arc<dyn ToolLedger>,
    ));
    server
        .gate()
        .set_allowed(ids.iter().map(|id| (*id).to_owned()));
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let host = Arc::new(McpHost::new(server, &data_dir(name)));
    (runtime, host)
}

/// Reads one newline-terminated response frame, bounded so a wedged host
/// fails the test rather than hanging it.
async fn read_frame(reader: &mut (impl tokio::io::AsyncRead + Unpin)) -> serde_json::Value {
    use tokio::io::AsyncBufReadExt;
    let mut line = Vec::new();
    let mut reader = tokio::io::BufReader::new(&mut *reader);
    tokio::time::timeout(Duration::from_secs(10), reader.read_until(b'\n', &mut line))
        .await
        .expect("a frame arrives within the bound")
        .expect("the frame reads");
    assert_eq!(line.pop(), Some(b'\n'), "the frame ends with the newline");
    serde_json::from_slice(&line).expect("one JSON value")
}

/// Enabling binds the endpoint and serves it: a client connects, the
/// handshake answers, and the allowlist seam decides what the wire
/// exposes.
#[test]
fn enabling_binds_and_serves_the_endpoint() {
    let (runtime, host) = host("enable", &["work"]);
    host.set_enabled(true);
    assert_eq!(host.status(), McpStatus::Listening, "the panel state");
    let endpoint = host.endpoint().clone();

    runtime.block_on(|| async move {
        use tokio::io::AsyncWriteExt;
        use tokio::net::windows::named_pipe::ClientOptions;

        let mut attempts = 200_u32;
        // Ride out the window where the first instance is being replaced,
        // bounded so a wedged host fails the test rather than hanging it.
        let stream = loop {
            assert!(
                attempts != 0,
                "the pipe never answered: a wedged host fails the test"
            );
            attempts -= 1;
            match ClientOptions::new().open(endpoint.as_str()) {
                Ok(stream) => break stream,
                Err(_) => tokio::time::sleep(Duration::from_millis(5)).await,
            }
        };
        let (mut reader, mut writer) = tokio::io::split(stream);
        writer
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
        writer.flush().await.expect("the handshake flushes");
        let hello = read_frame(&mut reader).await;
        assert_eq!(
            hello["result"]["serverInfo"]["name"], "astra-mail",
            "{hello}"
        );

        writer
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\
                 \"list_accounts\",\"arguments\":{}}}\n",
            )
            .await
            .expect("the call writes");
        writer.flush().await.expect("the call flushes");
        let accounts = read_frame(&mut reader).await;
        assert_eq!(
            accounts["result"]["structuredContent"]["accounts"]
                .as_array()
                .map(Vec::len),
            Some(1),
            "the seam's allowlist is the gate the wire sees: {accounts}",
        );
    });

    host.set_enabled(false);
    assert_eq!(host.status(), McpStatus::Off);
    runtime.shutdown_timeout(Duration::from_secs(5));
}

/// Disabling closes the endpoint: a client that dials after the stop is
/// refused, not left hanging.
#[test]
fn disabling_closes_the_endpoint() {
    let (runtime, host) = host("disable", &[]);
    host.set_enabled(true);
    assert_eq!(host.status(), McpStatus::Listening);
    host.set_enabled(false);
    assert_eq!(host.status(), McpStatus::Off);
    let endpoint = host.endpoint().clone();

    runtime.block_on(|| async move {
        use tokio::net::windows::named_pipe::ClientOptions;
        for _ in 0..200 {
            if ClientOptions::new().open(endpoint.as_str()).is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("the pipe must stop answering once the host is disabled");
    });
    runtime.shutdown_timeout(Duration::from_secs(5));
}

/// A taken endpoint name is the failed state the panel reads, not a
/// crash: the host keeps answering to further seam calls, and the
/// diagnostic names the bind failure.
#[test]
fn a_taken_name_is_a_failed_state_not_a_crash() {
    let (runtime, host) = host("taken", &[]);
    // Hold the name with a raw listener, the way another running instance
    // would. The pipe's creation needs a reactor, so the holder binds on
    // a runtime of its own that outlives it.
    let holder_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("the holder runtime");
    let holder = crate::runtime::drive(holder_runtime.handle(), || async {
        Listener::bind(host.endpoint()).expect("the holder binds")
    });
    host.set_enabled(true);
    let McpStatus::Failed(reason) = host.status() else {
        panic!("a taken name must read as failed, got {:?}", host.status());
    };
    assert!(!reason.is_empty(), "the diagnostic names the failure");
    // The host survives the failure and answers the seam afterwards.
    host.set_enabled(false);
    assert_eq!(host.status(), McpStatus::Off);
    drop(holder);
    drop(holder_runtime);
    host.set_enabled(true);
    assert_eq!(host.status(), McpStatus::Listening);
    host.set_enabled(false);
    runtime.shutdown_timeout(Duration::from_secs(5));
}

/// The allowlist seam reaches the gate a live connection answers through:
/// read fresh per call, not frozen at enable time.
#[test]
fn the_allowlist_seam_reaches_the_gate_on_a_live_connection() {
    let (runtime, host) = host("allowlist", &["work"]);
    host.set_enabled(true);
    let endpoint = host.endpoint().clone();

    runtime.block_on(|| {
        let host = Arc::clone(&host);
        async move {
            use tokio::io::AsyncWriteExt;
            use tokio::net::windows::named_pipe::ClientOptions;

            let mut attempts = 200_u32;

            // Bounded, like its sibling: a wedged host fails the test
            // rather than hanging it.
            let stream = loop {
                assert!(
                    attempts != 0,
                    "the pipe never answered: a wedged host fails the test"
                );
                attempts -= 1;
                match ClientOptions::new().open(endpoint.as_str()) {
                    Ok(stream) => break stream,
                    Err(_) => tokio::time::sleep(Duration::from_millis(5)).await,
                }
            };
            let (mut reader, mut writer) = tokio::io::split(stream);
            let call = |id: u8| {
                format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"tools/call\",\"params\":{{\"name\":\
                 \"list_accounts\",\"arguments\":{{}}}}}}\n"
            )
            };
            writer
                .write_all(call(1).as_bytes())
                .await
                .expect("the first call writes");
            writer.flush().await.expect("the first call flushes");
            let one = read_frame(&mut reader).await;
            assert_eq!(
                one["result"]["structuredContent"]["accounts"]
                    .as_array()
                    .map(Vec::len),
                Some(1),
                "the seam set before the connection: {one}",
            );

            // Empty the allowlist on the live connection: the next call
            // answers nothing.
            host.set_allowed(Vec::new());
            writer
                .write_all(call(2).as_bytes())
                .await
                .expect("the second call writes");
            writer.flush().await.expect("the second call flushes");
            let none = read_frame(&mut reader).await;
            assert_eq!(
                none["result"]["structuredContent"]["accounts"]
                    .as_array()
                    .map(Vec::len),
                Some(0),
                "the gate is read fresh on every call: {none}",
            );
        }
    });

    host.set_enabled(false);
    runtime.shutdown_timeout(Duration::from_secs(5));
}
