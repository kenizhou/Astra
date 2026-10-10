// SPDX-License-Identifier: GPL-3.0-only
//! The offline agent e2e: the real relay hosting the real sidecar bundle
//! under the mock harness, with astra-cli played by the committed stub
//! from the sidecar's fixtures. One scripted conversation runs the whole
//! loop: prompt, text deltas, a gated save_draft whose approval card is
//! answered, the stub CLI execution carrying the turn's correlation id,
//! the tool result, and done. The assertion that matters most is the
//! ledger's: the permission audit row, the tool audit row, and the
//! llm_call row all carry the turn's correlation id, and so does the
//! argv and environment the stub CLI actually received.
//!
//! Nothing here touches a network or a model: the mock harness streams
//! the committed transcript `mock-save-draft.json`, so the suite runs
//! on CI. The bundle builds in the setup when `sidecar/dist` is absent
//! (esbuild invoked through node; `npm --prefix sidecar ci` must have
//! run once); the test skips with a message when no node >= 22 exists.

use std::{path::PathBuf, sync::Arc, time::Duration};

use super::{
    default_policy,
    node::find_node,
    protocol::PermissionLevel,
    relay::SidecarConfig,
    spawn_sidecar,
    testkit::{self, CollectingSink},
    SidecarRelay, StreamPayload as S,
};
use astra_ai::{AiStore, CorrelationId};

/// The correlation id the scripted turn rides: fixed so every assertion
/// can name it, the way `agent_prompt` names a caller's id.
const CORR: &str = "corr-e2e-1";

/// One test's runtime, built the way the host builds its own (the crate's
/// clippy config forbids `#[tokio::test]`'s own `block_on`).
fn runtime() -> crate::runtime::AppRuntime {
    crate::runtime::build_runtime().expect("the test runtime builds")
}

/// The sidecar checkout beside the desktop crate.
fn sidecar_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sidecar")
}

/// The bundle the e2e hosts: `dist/index.cjs` when present, else built by
/// running the sidecar's esbuild config through the resolved node.
fn ensure_bundle(node: &std::path::Path) -> PathBuf {
    let bundle = sidecar_dir().join("dist").join("index.cjs");
    if bundle.is_file() {
        return bundle;
    }
    let output = std::process::Command::new(node)
        .arg("esbuild.config.mjs")
        .current_dir(sidecar_dir())
        .output()
        .expect("the bundle build spawns");
    assert!(
        output.status.success() && bundle.is_file(),
        "the sidecar bundle build failed (run npm --prefix sidecar ci once); stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    bundle
}

/// Stages the stub astra-cli from Task 4's fixtures: safe_exec runs the
/// init-delivered cli_path binary with the argv tail after the prefix
/// token, so with cli_path = node the verb positional (`drafts`) must
/// resolve to a script inside the cwd, which init pins at the data dir.
fn stage_stub_cli(data_dir: &std::path::Path) {
    let stub = include_str!("../../../../sidecar/src/fixtures/stub-cli.cjs");
    for verb in ["folders", "messages", "drafts"] {
        std::fs::write(data_dir.join(verb), stub).expect("the stub verb script writes");
    }
}

/// Waits for the predicate over the sink, up to ten seconds.
async fn until(predicate: impl Fn(&CollectingSink) -> bool, sink: &Arc<CollectingSink>) -> bool {
    for _ in 0..2000 {
        if predicate(sink) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    false
}

/// The one stub invocation the turn produced, from `stub-captures.jsonl`.
fn stub_capture(data_dir: &std::path::Path) -> serde_json::Value {
    let text =
        std::fs::read_to_string(data_dir.join("stub-captures.jsonl")).expect("the stub recorded");
    let lines: Vec<&str> = text.trim().lines().collect();
    assert_eq!(lines.len(), 1, "exactly one CLI invocation ran: {text}");
    serde_json::from_str(lines[0]).expect("the capture line parses")
}

#[test]
fn the_agent_loop_runs_offline_end_to_end() {
    let Ok(node) = find_node() else {
        // CI pins node 24; a host without any node >= 22 skips loudly
        // rather than failing on an environment the agent cannot fix.
        eprintln!(
            "e2e: skipping the offline agent loop: no node >= 22 on PATH or ASTRA_AGENT_NODE"
        );
        return;
    };
    let runtime = runtime();
    let bundle = ensure_bundle(&node);
    let data_dir = testkit::scratch("agent-e2e");
    stage_stub_cli(&data_dir);
    let transcript = sidecar_dir()
        .join("src")
        .join("fixtures")
        .join("mock-save-draft.json");
    assert!(transcript.is_file(), "the transcript fixture is committed");

    let sink = Arc::new(CollectingSink::default());
    let store = Arc::new(AiStore::open(&data_dir.join("ai.sqlite")).expect("the store opens"));
    let spawn = spawn_sidecar(node.clone(), bundle.clone(), data_dir.clone(), Vec::new());
    let relay = SidecarRelay::new(
        SidecarConfig {
            node_path: node.clone(),
            script: bundle,
            data_dir: data_dir.clone(),
            cli_path: node.to_string_lossy().into_owned(),
            harness: "mock".to_string(),
            mock_fixture: Some(transcript.to_string_lossy().into_owned()),
            model_configs: std::collections::BTreeMap::new(),
            policy: default_policy(),
            restart_max: 3,
            backoff_base_ms: 50,
            config_error: None,
            spawn,
        },
        sink.clone(),
        store,
    );

    runtime.block_on(|| async {
        let prompt_id = relay
            .prompt(
                "Draft a short reply to Bob accepting Friday.",
                Some(CorrelationId::new(CORR.to_owned())),
            )
            .await
            .expect("the prompt is sent");

        // The gated save_draft surfaces its approval card before anything
        // executes, carrying the turn's correlation id.
        let asked = until(|s| !s.requests.lock().unwrap().is_empty(), &sink).await;
        assert!(asked, "the gated save_draft asks before executing");
        let ask = sink.requests.lock().unwrap()[0].clone();
        assert_eq!(ask.tool, "save_draft");
        assert_eq!(ask.level, PermissionLevel::Confirm);
        assert_eq!(ask.correlation.as_deref(), Some(CORR));
        assert!(
            !data_dir.join("stub-captures.jsonl").exists(),
            "nothing executed while the ask was open"
        );

        relay
            .respond_permission(&ask.id, true, false)
            .await
            .expect("the approval rides back");

        // state, two deltas, tool_call, tool_result, state, done: the
        // llm_call envelope bypasses the stream into the ledger.
        let events = sink.await_events(7).await;
        assert_eq!(
            events.len(),
            7,
            "the stream holds exactly the turn: {events:?}"
        );
        for event in &events {
            assert_eq!(
                event.correlation.as_deref(),
                Some(CORR),
                "every event carries the turn's correlation id: {event:?}"
            );
        }
        assert!(matches!(&events[0].event, S::State { state } if state == "thinking"));
        assert!(matches!(&events[1].event, S::TextDelta { text } if text.contains("draft that reply")));
        assert!(matches!(&events[2].event, S::TextDelta { text } if text.contains("Saving the draft")));
        assert!(matches!(
            &events[3].event,
            S::ToolCall { name, tool_call_id, .. } if name == "save_draft" && tool_call_id == "mock-tc-1"
        ));
        match &events[4].event {
            S::ToolResult {
                tool_call_id,
                name,
                status,
                output,
            } => {
                assert_eq!(tool_call_id, "mock-tc-1");
                assert_eq!(name, "save_draft");
                assert_eq!(status, "ok");
                let output = output.as_deref().unwrap_or_default();
                assert!(
                    output.contains("\"script\":\"drafts\"")
                        && output.contains("--correlation-id"),
                    "the tool result carries the stub CLI's echo: {output}"
                );
            }
            other => panic!("the fifth event is the tool result, not {other:?}"),
        }
        assert!(matches!(&events[5].event, S::State { state } if state == "done-streaming"));
        assert!(matches!(&events[6].event, S::Done { prompt_id: id } if id == &prompt_id));

        // The settled approval reached the UI beside the ask.
        let resolved = sink.resolved.lock().unwrap();
        assert_eq!(resolved.len(), 1, "the approval settles once: {resolved:?}");
        assert_eq!(resolved[0].id, ask.id);
        assert!(resolved[0].approved);
        drop(resolved);
    });

    // The ledger's core claim: the permission decision's audit row, the
    // tool result's audit row, and the llm_call row share the turn's
    // correlation id.
    let audits = testkit::audit_rows(&data_dir);
    let permission_row = (
        CORR.to_string(),
        "permission save_draft".to_string(),
        "approved".to_string(),
    );
    let tool_row = (CORR.to_string(), "save_draft".to_string(), "ok".to_string());
    assert!(
        audits.contains(&permission_row),
        "the approval decision is audited: {audits:?}"
    );
    assert!(
        audits.contains(&tool_row),
        "the tool outcome is audited: {audits:?}"
    );
    let rows = testkit::llm_rows(&data_dir);
    assert_eq!(rows.len(), 1, "one llm_call row per turn: {rows:?}");
    assert_eq!(rows[0].0, CORR, "the llm_call row shares the turn's id");
    assert_eq!(rows[0].1, "mock-1");
    assert_eq!(
        rows[0].2, "chat",
        "the mock harness records at the chat tier"
    );
    assert_eq!(rows[0].3, "assistant_chat");
    assert!(
        rows[0].4 > 0 && rows[0].5 > 0,
        "usage tokens are real: {rows:?}"
    );

    // The correlation id rode the actual CLI spawn: argv flag, injected
    // environment, and the pinned cwd.
    let capture = stub_capture(&data_dir);
    let argv = capture["argv"].as_array().expect("the capture argv");
    let argv: Vec<&str> = argv
        .iter()
        .map(|v| v.as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        argv,
        [
            "drafts",
            "save",
            "--json",
            "--correlation-id",
            CORR,
            "--account",
            "acct-1",
            "--from",
            "ada@example.com",
            "--to",
            "bob@example.com",
            "--subject",
            "Re: Friday",
            "--plain",
            "See you then."
        ],
        "the stub saw the pinned argv grammar"
    );
    assert_eq!(capture["correlation_id"].as_str(), Some(CORR));
    assert_eq!(
        capture["cwd"].as_str(),
        Some(data_dir.to_string_lossy().as_ref()),
        "the CLI child runs inside the data dir"
    );

    runtime.block_on(|| async { relay.shutdown().await });
    assert_eq!(relay.status().as_str(), "down", "the shutdown settles");
}
