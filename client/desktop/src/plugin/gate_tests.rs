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
use astra_plugin::Permission;
use engine_api::{AccountId, EmailAddress};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, DuplexStream};

use crate::plugin::{testkit, PluginGate, PluginRegistry};

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
        let registry = testkit::registry_with(
            "allow",
            "dev.astra.gate",
            &["before_send", "composer.transform"],
            Some("block"),
            &[Permission::Send, Permission::Admin],
            testkit::spawn_echo(),
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
            let registry = testkit::registry_with(
                &format!("block-{mode}"),
                "dev.astra.gate",
                &["before_send", "composer.transform"],
                Some(mode),
                &[Permission::Send, Permission::Admin],
                testkit::spawn_of(scripted_worker(Script::Block, seen)),
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
            let registry = testkit::registry_with(
                &format!("down-{mode}"),
                "dev.astra.gate",
                &["before_send", "composer.transform"],
                Some(mode),
                &[Permission::Send, Permission::Admin],
                testkit::spawn_echo(),
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
            let registry = testkit::registry_with(
                &format!("slow-{mode}"),
                "dev.astra.gate",
                &["before_send", "composer.transform"],
                Some(mode),
                &[Permission::Send, Permission::Admin],
                testkit::spawn_of(scripted_worker(Script::Hang, Arc::new(Mutex::new(0)))),
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
        let registry = testkit::registry_with(
            "malformed",
            "dev.astra.gate",
            &["before_send", "composer.transform"],
            Some("block"),
            &[Permission::Send, Permission::Admin],
            testkit::spawn_of(scripted_worker(Script::Malformed, Arc::new(Mutex::new(0)))),
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
        let registry = testkit::registry_with(
            "no-admin",
            "dev.astra.gate",
            &["before_send", "composer.transform"],
            Some("block"),
            &[Permission::Send], // declared, granted; admin withheld
            testkit::spawn_of(scripted_worker(Script::Block, Arc::clone(&seen))),
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
        let registry = testkit::registry_with(
            "transform",
            "dev.astra.gate",
            &["before_send", "composer.transform"],
            Some("block"),
            &[Permission::Send, Permission::Admin, Permission::ComposeEdit],
            testkit::spawn_echo(),
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
        let registry = testkit::registry_with(
            "transform-down",
            "dev.astra.gate",
            &["before_send", "composer.transform"],
            Some("block"),
            &[Permission::Send, Permission::Admin, Permission::ComposeEdit],
            testkit::spawn_echo(),
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
