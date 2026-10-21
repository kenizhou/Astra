// SPDX-License-Identifier: GPL-3.0-only
//! The echo plugin: the worker protocol's conformance fixture.
//!
//! It answers `plugin_init` with a ready naming its manifest's methods,
//! echoes every `object_call`'s args back as the result, acknowledges a
//! subscription with one event on the stream, and exits on
//! `plugin_shutdown`. It ships in the repo so the worker host's tests and the
//! send gate's harness smoke have a real counterparty; it never ships in a
//! build.

use astra_plugin::{HostMessage, WorkerMessage, AOM_VERSION, PLUGIN_API_LEVEL};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// Runs the echo loop over the given pipe ends until `plugin_shutdown` or
/// EOF. Any bytes the host never framed are ignored: the fixture's job is
/// conformance, not suspicion.
///
/// # Errors
///
/// Returns the pipe's IO error when a read or write fails, and serde's error
/// when an answer fails to serialise (which, for these shapes, it never
/// should).
pub async fn run(
    reader: impl AsyncBufRead + Unpin,
    mut writer: impl AsyncWrite + Unpin,
) -> std::io::Result<()> {
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        let message: HostMessage = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(_) => continue,
        };
        let answer = match message {
            HostMessage::PluginInit { manifest, .. } => WorkerMessage::PluginReady {
                api_level: PLUGIN_API_LEVEL,
                aom_version: AOM_VERSION,
                methods: manifest.methods,
            },
            HostMessage::ObjectCall {
                id, method, args, ..
            } => WorkerMessage::ObjectResult {
                id,
                value: answer_for(&method, args),
            },
            HostMessage::ObjectSubscribe { stream, .. } => WorkerMessage::ObjectEvent {
                stream,
                event: serde_json::json!({ "echo": "subscribed" }),
            },
            HostMessage::PluginShutdown => return Ok(()),
        };
        let mut line = serde_json::to_string(&answer)?;
        line.push('\n');
        writer.write_all(line.as_bytes()).await?;
        writer.flush().await?;
    }
    Ok(())
}

/// The echo's answer for one call: the send gate gets its allow verdict, the
/// compose transform gets its own bodies back in the response's shape, and
/// every other method gets its args verbatim.
fn answer_for(method: &str, args: serde_json::Value) -> serde_json::Value {
    match method {
        "before_send" => serde_json::json!({ "verdict": "allow" }),
        "composer.transform" => serde_json::json!({
            "html": args["composer_html"],
            "plain": args["composer_plain"],
        }),
        _ => args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::io::duplex;

    /// Runs the echo loop on the worker end of a duplex pair and returns the
    /// host end for the test to speak into.
    fn start() -> (
        tokio::io::DuplexStream,
        tokio::task::JoinHandle<std::io::Result<()>>,
    ) {
        let (host_end, worker_end) = duplex(4096);
        let (worker_read, worker_write) = tokio::io::split(worker_end);
        let task = tokio::spawn(run(tokio::io::BufReader::new(worker_read), worker_write));
        (host_end, task)
    }

    async fn say(stream: &mut tokio::io::DuplexStream, message: serde_json::Value) {
        let mut line = serde_json::to_string(&message).expect("serialises");
        line.push('\n');
        stream.write_all(line.as_bytes()).await.expect("written");
        stream.flush().await.expect("flushed");
    }

    async fn hear(stream: &mut tokio::io::DuplexStream) -> serde_json::Value {
        let mut lines = tokio::io::BufReader::new(stream).lines();
        let line = lines.next_line().await.expect("read").expect("a line");
        serde_json::from_str(&line).expect("valid JSON")
    }

    #[tokio::test]
    async fn init_ready_call_echo_and_shutdown() {
        let (mut host, task) = start();
        say(
            &mut host,
            serde_json::json!({
                "type": "plugin_init",
                "manifest": {
                    "id": "dev.astra.echo",
                    "name": "Echo",
                    "version": "0.1.0",
                    "api_level": 1,
                    "aom_version": 1,
                    "methods": ["plugin.dev.astra.echo.ping"],
                    "permissions": [],
                },
                "grants": [],
                "api_level": 1,
                "aom_version": 1,
                "data_dir": "C:/plugins/echo",
            }),
        )
        .await;
        let ready = hear(&mut host).await;
        assert_eq!(ready["type"], "plugin_ready");
        assert_eq!(
            ready["methods"],
            serde_json::json!(["plugin.dev.astra.echo.ping"])
        );

        say(
            &mut host,
            serde_json::json!({
                "type": "object_call",
                "id": "c1",
                "method": "plugin.dev.astra.echo.ping",
                "args": { "n": 1 },
            }),
        )
        .await;
        let result = hear(&mut host).await;
        assert_eq!(
            result,
            serde_json::json!({ "type": "object_result", "id": "c1", "value": { "n": 1 } }),
            "the echo returned the args verbatim"
        );

        say(
            &mut host,
            serde_json::json!({
                "type": "object_subscribe",
                "id": "s1",
                "stream": "explorer.onSelectionChange",
            }),
        )
        .await;
        let event = hear(&mut host).await;
        assert_eq!(event["type"], "object_event");
        assert_eq!(event["stream"], "explorer.onSelectionChange");

        say(
            &mut host,
            serde_json::json!({
                "type": "object_call",
                "id": "c2",
                "method": "before_send",
                "args": { "account": "work" },
            }),
        )
        .await;
        let verdict = hear(&mut host).await;
        assert_eq!(
            verdict["value"],
            serde_json::json!({ "verdict": "allow" }),
            "the gate gets its allow verdict"
        );

        say(
            &mut host,
            serde_json::json!({
                "type": "object_call",
                "id": "c3",
                "method": "composer.transform",
                "args": { "composer_html": "<p>hi</p>", "composer_plain": "hi" },
            }),
        )
        .await;
        let transformed = hear(&mut host).await;
        assert_eq!(
            transformed["value"],
            serde_json::json!({ "html": "<p>hi</p>", "plain": "hi" }),
            "the transform answers in the response's shape"
        );

        say(&mut host, serde_json::json!({ "type": "plugin_shutdown" })).await;
        task.await.expect("no panic").expect("clean exit");
    }
}
