// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the worker host: the handshake, the call round trip, the
//! supervision machine, and the fail-closed drains. Every test drives a
//! scripted in-memory worker over duplex streams, so the protocol and the
//! machine are exercised without a real process; the real spawn path is
//! covered by the echo plugin's own binary smoke.

use std::time::Duration;

use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, DuplexStream};

use super::*;

use crate::RouteError;

/// The handshake and call behaviour of a conforming worker, scripted onto one
/// end of the duplex pair.
async fn conforming_worker(mut stream: DuplexStream, die_after_ready: bool) {
    let (read, mut write) = tokio::io::split(&mut stream);
    let mut lines = tokio::io::BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let message: serde_json::Value = serde_json::from_str(&line).expect("valid JSON");
        let answer = match message["type"].as_str() {
            Some("plugin_init") => serde_json::json!({
                "type": "plugin_ready",
                "api_level": PLUGIN_API_LEVEL,
                "aom_version": AOM_VERSION,
                "methods": ["plugin.dev.astra.echo.ping", "message.markRead"],
            }),
            Some("object_call") => serde_json::json!({
                "type": "object_result",
                "id": message["id"],
                "value": message["args"],
            }),
            Some("object_subscribe") => {
                continue;
            }
            Some("plugin_shutdown") => return,
            _ => continue,
        };
        let mut line = serde_json::to_string(&answer).expect("serialises");
        line.push('\n');
        write
            .write_all(line.as_bytes())
            .await
            .expect("answer written");
        write.flush().await.expect("flushed");
        if die_after_ready && message["type"] == "plugin_init" {
            // Die shortly after ready: long enough for the host's first
            // writes to land, then the stream closes and the pump sees EOF.
            tokio::time::sleep(Duration::from_millis(50)).await;
            return;
        }
    }
}

/// A spawn function wiring the host to a scripted worker over a duplex pair.
fn spawn_with<F, Fut>(worker: F) -> (SpawnFn, tokio::sync::mpsc::UnboundedReceiver<()>)
where
    F: Fn(DuplexStream) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (spawned_tx, spawned_rx) = tokio::sync::mpsc::unbounded_channel();
    let spawn: SpawnFn = std::sync::Arc::new(move || {
        let (host_end, worker_end) = duplex(4096);
        let (read, write) = tokio::io::split(host_end);
        let spawned_tx = spawned_tx.clone();
        let _ = spawned_tx.send(());
        tokio::spawn(worker(worker_end));
        Box::pin(async move {
            Ok(WorkerPipes {
                reader: Box::new(tokio::io::BufReader::new(read)),
                writer: Box::new(write),
                killer: Box::new(|| Box::pin(async {})),
            })
        })
    });
    (spawn, spawned_rx)
}

fn manifest() -> PluginManifest {
    PluginManifest::parse(
        r#"
id = "dev.astra.echo"
name = "Echo"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = ["plugin.dev.astra.echo.ping", "message.markRead"]
permissions = ["read_mailbox"]
"#,
    )
    .expect("manifest parses")
}

fn config() -> WorkerConfig {
    WorkerConfig {
        program: "echo".into(),
        args: vec![],
        data_dir: "C:/plugins/echo".into(),
        init_timeout: Duration::from_millis(200),
        restart_max: 2,
        backoff_base: Duration::from_millis(10),
    }
}

fn host(spawn: SpawnFn) -> WorkerHost {
    WorkerHost::new(
        config(),
        manifest(),
        &[Permission::ReadMailbox],
        true,
        spawn,
    )
}

#[tokio::test]
async fn the_handshake_sends_init_and_reads_ready() {
    let (spawn, mut spawned) = spawn_with(|stream| conforming_worker(stream, false));
    let host = host(spawn);
    host.start().await.expect("handshake succeeds");
    assert_eq!(host.state(), WorkerState::Ready);
    assert!(spawned.try_recv().is_ok(), "the spawn function ran");
    host.shutdown().await;
}

#[tokio::test]
async fn a_version_mismatch_in_the_ready_is_refused() {
    let (spawn, _) = spawn_with(|mut stream| async move {
        let (read, mut write) = tokio::io::split(&mut stream);
        let mut lines = tokio::io::BufReader::new(read).lines();
        let _ = lines.next_line().await;
        let answer = serde_json::json!({
            "type": "plugin_ready",
            "api_level": PLUGIN_API_LEVEL + 1,
            "aom_version": AOM_VERSION,
            "methods": [],
        });
        let mut line = serde_json::to_string(&answer).expect("serialises");
        line.push('\n');
        write.write_all(line.as_bytes()).await.expect("written");
    });
    let host = host(spawn);
    let error = host.start().await.expect_err("mismatch refused");
    assert!(matches!(error, WorkerError::VersionMismatch { .. }));
    assert_eq!(host.state(), WorkerState::Lost);
}

#[tokio::test]
async fn an_init_timeout_lands_in_lost_after_the_budget() {
    let (spawn, _) = spawn_with(|stream| async move {
        // The worker never answers: the init timeout fires. The stream must
        // be held, or dropping it would close the pipe and read as an exit.
        let _hold = stream;
        std::future::pending::<()>().await;
    });
    let host = host(spawn);
    let error = host.start().await.expect_err("init times out");
    assert!(matches!(error, WorkerError::InitTimeout));
    assert_eq!(host.state(), WorkerState::Lost);
}

#[tokio::test]
async fn a_call_round_trips_through_the_router_and_the_pipe() {
    let (spawn, _) = spawn_with(|stream| conforming_worker(stream, false));
    let host = host(spawn);
    host.start().await.expect("ready");
    let handle = host
        .mint_handle(HandleKind::Message, "work", "imap:v1:u1")
        .await;
    let value = host
        .call(Some(handle), "message.markRead", serde_json::json!([true]))
        .await
        .expect("call answers");
    assert_eq!(
        value,
        serde_json::json!([true]),
        "the echo returned the args"
    );
    host.shutdown().await;
}

#[tokio::test]
async fn the_router_refuses_before_the_pipe() {
    let (spawn, _) = spawn_with(|stream| conforming_worker(stream, false));
    let host = host(spawn);
    host.start().await.expect("ready");
    let error = host
        .call(None, "message.send", serde_json::json!([]))
        .await
        .expect_err("undeclared and ungranted");
    assert!(matches!(
        error,
        WorkerError::Refused(RouteError::UndeclaredMethod(_))
    ));
    host.shutdown().await;
}

#[tokio::test]
async fn a_worker_death_errors_pending_calls_and_drains_subscriptions() {
    let (spawn, _) = spawn_with(|stream| conforming_worker(stream, true));
    let host = host(spawn);
    host.start().await.expect("ready");
    host.subscribe("explorer.onSelectionChange")
        .await
        .expect("subscribed");
    // The scripted worker died right after ready; the pump's EOF lands here.
    host.call(None, "plugin.dev.astra.echo.ping", serde_json::json!([]))
        .await
        .expect_err("the dead worker cannot answer");
    // The restart budget covers two attempts; the same dying worker script
    // exhausts it and the host lands in Lost.
    for _ in 0..50 {
        if host.state() == WorkerState::Lost {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(host.state(), WorkerState::Lost);
    assert_eq!(
        host.subscription_count().await,
        0,
        "death drained the subscriptions"
    );
}

#[tokio::test]
async fn a_restarted_worker_gets_a_fresh_generation() {
    let (spawn, mut spawned) = spawn_with(|stream| conforming_worker(stream, false));
    let host = host(spawn);
    host.start().await.expect("ready");
    let first = host.generation().await;
    host.start().await.expect("manual restart");
    let second = host.generation().await;
    assert!(second > first, "a restart mints a new generation");
    assert!(spawned.try_recv().is_ok());
    assert!(spawned.try_recv().is_ok(), "the spawn function ran twice");
    host.shutdown().await;
}

#[tokio::test]
async fn shutdown_asks_then_kills() {
    let (spawn, _) = spawn_with(|stream| conforming_worker(stream, false));
    let host = host(spawn);
    host.start().await.expect("ready");
    host.shutdown().await;
    assert_eq!(host.state(), WorkerState::Down);
    // The worker is gone: a call after shutdown fails fast.
    let error = host
        .call(None, "plugin.dev.astra.echo.ping", serde_json::json!([]))
        .await
        .expect_err("down host");
    assert!(matches!(error, WorkerError::NotReady));
}
