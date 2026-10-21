// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the desktop's plugin gate: the fold of every serving worker's
//! answer into the one verdict the app sees, the mode semantics on an
//! unavailable or slow worker, and the admin-grant check that keeps a
//! non-admin plugin out of the send path entirely.
//!
//! The workers are real: the echo plugin runs in-process over duplex streams
//! through the worker host's custom spawn path, and the scripted workers drive
//! the modes the echo does not know.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use astra_app::{BeforeSendGate, ComposeTransform, GateComposition, GateVerdict};
use astra_plugin::{Permission, SpawnFn};
use engine_api::{AccountId, EmailAddress};
use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, DuplexStream};

use super::{PluginGate, PluginRegistry};

/// The verdict a scripted worker answers `before_send` with, or hangs. The
/// allow path is the echo plugin's job (the conformance counterparty), so the
/// script set covers only what echo does not know.
#[derive(Clone, Copy)]
enum Script {
    Block,
    Hang,
    Malformed,
}

/// A scripted worker speaking the envelope: ready, then the scripted verdict
/// per call (a hang never answers).
fn scripted_worker(
    script: Script,
    seen: Arc<Mutex<usize>>,
) -> impl Fn(DuplexStream) -> tokio::task::JoinHandle<()> {
    move |stream| {
        let seen = Arc::clone(&seen);
        tokio::spawn(async move {
            let (read, mut write) = tokio::io::split(stream);
            let mut lines = tokio::io::BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let message: serde_json::Value = serde_json::from_str(&line).expect("valid JSON");
                let answer = match message["type"].as_str() {
                    Some("plugin_init") => serde_json::json!({
                        "type": "plugin_ready",
                        "api_level": 1,
                        "aom_version": 1,
                        "methods": ["before_send"],
                    }),
                    Some("object_call") => {
                        *seen.lock().unwrap() += 1;
                        match script {
                            Script::Block => serde_json::json!({
                                "type": "object_result", "id": message["id"],
                                "value": { "verdict": "block", "reason": "recipient is not trusted" },
                            }),
                            Script::Hang => continue,
                            Script::Malformed => serde_json::json!({
                                "type": "object_result", "id": message["id"],
                                "value": { "unexpected": true },
                            }),
                        }
                    }
                    Some("plugin_shutdown") => return,
                    _ => continue,
                };
                let mut line = serde_json::to_string(&answer).expect("serialises");
                line.push('\n');
                write.write_all(line.as_bytes()).await.expect("written");
                write.flush().await.expect("flushed");
            }
        })
    }
}

/// A spawn function driving one scripted worker over a duplex pair.
fn spawn_of(
    worker: impl Fn(DuplexStream) -> tokio::task::JoinHandle<()> + Send + Sync + 'static,
) -> SpawnFn {
    Arc::new(move || {
        let (host_end, worker_end) = duplex(4096);
        let (read, write) = tokio::io::split(host_end);
        worker(worker_end);
        Box::pin(async move {
            Ok(astra_plugin::WorkerPipes {
                reader: Box::new(tokio::io::BufReader::new(read)),
                writer: Box::new(write),
                killer: Box::new(|| Box::pin(async {})),
            })
        })
    })
}

/// The echo plugin over a duplex pair: the conformance counterparty itself.
fn spawn_echo() -> SpawnFn {
    spawn_of(|stream| {
        let (read, write) = tokio::io::split(stream);
        tokio::spawn(async move {
            let _ = astra_plugin_echo::run(tokio::io::BufReader::new(read), write).await;
        })
    })
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "astra-desktop-gate-{}-{}",
        name,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// A registry holding one plugin with the given mode and grants, its manifest
/// written under the scratch root.
async fn registry_with(
    name: &str,
    mode: &str,
    grants: &[Permission],
    spawn: SpawnFn,
) -> Arc<PluginRegistry> {
    let root = scratch(name);
    let dir = root.join("dev.astra.gate");
    std::fs::create_dir_all(&dir).expect("plugin dir");
    std::fs::write(
        dir.join("manifest.toml"),
        // Every permission the methods need is declared; the grant table
        // (what `grants` is approved into below) is what the gate checks.
        format!(
            r#"
id = "dev.astra.gate"
name = "Gate"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = ["before_send", "composer.transform"]
permissions = ["send", "admin", "compose_edit"]
before_send_mode = "{mode}"
"#
        ),
    )
    .expect("manifest written");
    let registry = Arc::new(PluginRegistry::new(root, vec![], true));
    registry.list();
    for grant in grants {
        registry
            .approve("dev.astra.gate", *grant)
            .expect("declared grant");
    }
    registry
        .enable_with("dev.astra.gate", spawn)
        .await
        .expect("the plugin enables");
    registry
}

fn composition() -> GateComposition {
    GateComposition {
        account: AccountId::try_from("work").unwrap(),
        from: EmailAddress::new("me@work.local"),
        to: vec![EmailAddress::new("ada@example.test")],
        cc: vec![],
        bcc: vec![],
        subject: "Quarterly report".to_owned(),
        plain: "Agreed".to_owned(),
        html: "<p>Agreed</p>".to_owned(),
    }
}

fn gate(registry: &Arc<PluginRegistry>) -> PluginGate {
    PluginGate::new(Arc::clone(registry), Duration::from_millis(80))
}

#[test]
fn the_echo_plugins_allow_folds_to_allow() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let registry = registry_with(
            "allow",
            "block",
            &[Permission::Send, Permission::Admin],
            spawn_echo(),
        )
        .await;
        let gate = gate(&registry);
        assert_eq!(gate.check(&composition()).await, GateVerdict::Allow);
    });
}

#[test]
fn a_block_verdict_blocks_and_a_prompt_mode_prompts() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        for (mode, expected) in [
            (
                "block",
                GateVerdict::Block {
                    plugin: "dev.astra.gate".to_owned(),
                    reason: "recipient is not trusted".to_owned(),
                },
            ),
            (
                "soft_block",
                GateVerdict::Block {
                    plugin: "dev.astra.gate".to_owned(),
                    reason: "recipient is not trusted".to_owned(),
                },
            ),
            (
                "prompt_user",
                GateVerdict::PromptUser {
                    plugin: "dev.astra.gate".to_owned(),
                    reason: "recipient is not trusted".to_owned(),
                },
            ),
        ] {
            let seen = Arc::new(Mutex::new(0));
            let registry = registry_with(
                &format!("block-{mode}"),
                mode,
                &[Permission::Send, Permission::Admin],
                spawn_of(scripted_worker(Script::Block, seen)),
            )
            .await;
            let gate = gate(&registry);
            assert_eq!(gate.check(&composition()).await, expected, "mode {mode}");
        }
    });
}

#[test]
fn an_unavailable_worker_blocks_block_mode_and_fails_the_rest_open() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        for (mode, expected) in [
            (
                "block",
                GateVerdict::Block {
                    plugin: "dev.astra.gate".to_owned(),
                    reason: "the plugin is unavailable".to_owned(),
                },
            ),
            ("soft_block", GateVerdict::Allow),
            ("prompt_user", GateVerdict::Allow),
        ] {
            let registry = registry_with(
                &format!("down-{mode}"),
                mode,
                &[Permission::Send, Permission::Admin],
                spawn_echo(),
            )
            .await;
            // The worker dies: the gate must answer from the mode, not hang.
            registry
                .worker_host("dev.astra.gate")
                .expect("host")
                .shutdown()
                .await;
            let gate = gate(&registry);
            assert_eq!(gate.check(&composition()).await, expected, "mode {mode}");
        }
    });
}

#[test]
fn a_slow_handler_times_out_by_its_mode() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        for (mode, blocked) in [
            ("block", true),
            ("soft_block", false),
            ("prompt_user", false),
        ] {
            let registry = registry_with(
                &format!("slow-{mode}"),
                mode,
                &[Permission::Send, Permission::Admin],
                spawn_of(scripted_worker(Script::Hang, Arc::new(Mutex::new(0)))),
            )
            .await;
            let gate = gate(&registry);
            let verdict = gate.check(&composition()).await;
            assert_eq!(
                matches!(verdict, GateVerdict::Block { .. }),
                blocked,
                "mode {mode} times out {}",
                if blocked { "closed" } else { "open" },
            );
        }
    });
}

#[test]
fn a_malformed_answer_is_an_error_not_a_verdict() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let registry = registry_with(
            "malformed",
            "block",
            &[Permission::Send, Permission::Admin],
            spawn_of(scripted_worker(Script::Malformed, Arc::new(Mutex::new(0)))),
        )
        .await;
        let gate = gate(&registry);
        assert!(
            matches!(gate.check(&composition()).await, GateVerdict::Block { .. }),
            "a malformed answer in block mode fails closed"
        );
    });
}

#[test]
fn a_plugin_without_the_admin_grant_is_never_called() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let seen = Arc::new(Mutex::new(0));
        let registry = registry_with(
            "no-admin",
            "block",
            &[Permission::Send], // declared, granted; admin withheld
            spawn_of(scripted_worker(Script::Block, Arc::clone(&seen))),
        )
        .await;
        let gate = gate(&registry);
        assert_eq!(gate.check(&composition()).await, GateVerdict::Allow);
        assert_eq!(
            *seen.lock().unwrap(),
            0,
            "auto-activation without the admin grant never fires"
        );
    });
}

#[test]
fn the_transform_round_trips_through_the_echo_plugin() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let registry = registry_with(
            "transform",
            "block",
            &[Permission::Send, Permission::Admin, Permission::ComposeEdit],
            spawn_echo(),
        )
        .await;
        let gate = gate(&registry);
        let rewritten = gate
            .transform(composition())
            .await
            .expect("the echo answers in shape");
        assert_eq!(rewritten.plain, "Agreed");
        assert_eq!(rewritten.html, "<p>Agreed</p>");
    });
}

#[test]
fn a_failing_transform_worker_fails_closed() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let registry = registry_with(
            "transform-down",
            "block",
            &[Permission::Send, Permission::Admin, Permission::ComposeEdit],
            spawn_echo(),
        )
        .await;
        registry
            .worker_host("dev.astra.gate")
            .expect("host")
            .shutdown()
            .await;
        let gate = gate(&registry);
        let failure = gate
            .transform(composition())
            .await
            .expect_err("the dead worker cannot transform");
        assert_eq!(failure.plugin, "dev.astra.gate");
    });
}
