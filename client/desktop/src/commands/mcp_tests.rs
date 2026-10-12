// SPDX-License-Identifier: GPL-3.0-only
//! The mcp command tests: the status pull answers the host the boot
//! assembled (off and exposing nothing on a fresh one), the enable and
//! allowlist sets thread the host's own seams, the failed bind names
//! itself in the view the panel renders, and the snippet derivation
//! spells the relay command a client config pastes.

use std::path::Path;

use super::{relay_command, McpStateWord, McpStatusView};
use crate::app_state::AppState;
use crate::commands_fakes::{self, FakeApp, FakeStore};

/// A state whose mcp host stands over a scratch directory whose leaf
/// names the test, so an enabling test binds an endpoint no sibling
/// test holds.
fn state(name: &str) -> AppState {
    commands_fakes::state(name, FakeApp::new(), FakeStore::accepting())
}

#[test]
fn the_status_pull_answers_a_fresh_host_as_off_and_exposing_nothing() {
    let state = state("mcp-fresh");
    let view = super::mcp_status_of(&state);

    assert_eq!(view.status, McpStateWord::Off);
    assert!(view.reason.is_none(), "no diagnostic when nothing failed");
    assert!(
        view.allowed.is_empty(),
        "the gate allows nothing by default"
    );
    assert!(
        !view.endpoint.is_empty(),
        "the pull carries the endpoint the snippet needs"
    );
}

#[test]
fn the_allowlist_set_threads_the_gate_the_pull_reads() {
    let state = state("mcp-allowlist");
    super::mcp_set_allowed_of(&state, vec!["work".to_owned(), "home".to_owned()]);
    assert_eq!(
        super::mcp_status_of(&state).allowed,
        vec!["home".to_owned(), "work".to_owned()],
        "the set reaches the gate, which answers in its stable order",
    );

    // Replacing, never merging: the second set is the whole truth.
    super::mcp_set_allowed_of(&state, vec!["home".to_owned()]);
    assert_eq!(
        super::mcp_status_of(&state).allowed,
        vec!["home".to_owned()],
        "a later set replaces the standing allowlist",
    );
}

/// Enabling binds the real endpoint: the panel state moves to
/// listening, and disabling closes it again.
#[cfg(windows)]
#[test]
fn the_enable_set_threads_the_run_stop_seam() {
    let state = state("mcp-enable");
    super::mcp_set_enabled_of(&state, true);
    assert_eq!(
        super::mcp_status_of(&state).status,
        McpStateWord::Listening,
        "turning on binds and serves the endpoint",
    );
    super::mcp_set_enabled_of(&state, false);
    assert_eq!(
        super::mcp_status_of(&state).status,
        McpStateWord::Off,
        "turning off closes the endpoint",
    );
}

/// A taken endpoint name is the failed state the panel reads, with the
/// diagnostic riding the view's reason field.
#[cfg(windows)]
#[test]
fn a_taken_endpoint_names_the_failure_in_the_view() {
    let state = state("mcp-taken");
    // Hold the name with a raw listener, the way another running
    // instance would; the pipe's creation needs a reactor, so the
    // holder binds on a runtime of its own that outlives it.
    let holder_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("the holder runtime");
    let endpoint = state.mcp.endpoint().clone();
    let holder = crate::runtime::drive(holder_runtime.handle(), || async {
        astra_mcp::transport::Listener::bind(&endpoint).expect("the holder binds")
    });

    super::mcp_set_enabled_of(&state, true);
    let view = super::mcp_status_of(&state);
    assert_eq!(view.status, McpStateWord::Failed);
    assert!(
        view.reason
            .as_deref()
            .is_some_and(|reason| !reason.is_empty()),
        "the diagnostic names why the bind failed",
    );

    drop(holder);
    drop(holder_runtime);
    super::mcp_set_enabled_of(&state, false);
}

#[test]
fn the_snippet_names_the_relay_beside_the_app_with_the_endpoint_as_argv() {
    let exe = if cfg!(windows) {
        Path::new(r"C:\Apps\Astra Mail\astra.exe")
    } else {
        Path::new("/opt/astra mail/astra")
    };
    let endpoint = r"\\.\pipe\com.kenizhou.astra.mcp";
    let command = relay_command(exe, endpoint);

    let relay = if cfg!(windows) {
        r"C:\Apps\Astra Mail\astra-mcp-relay.exe"
    } else {
        "/opt/astra mail/astra-mcp-relay"
    };
    assert_eq!(
        command,
        vec![
            relay.to_owned(),
            "--endpoint".to_owned(),
            endpoint.to_owned(),
        ],
        "the argv is the relay beside the app plus the endpoint pair its usage line spells",
    );
}

/// The wire's own shape: an off view crosses with no reason key at all,
/// and the state words cross as the snake_case words the web reads.
#[test]
fn the_view_serialises_with_the_words_and_omissions_the_wire_reads() {
    let view = McpStatusView {
        status: McpStateWord::Off,
        reason: None,
        endpoint: "\\\\.\\pipe\\astra.mcp".to_owned(),
        allowed: Vec::new(),
        relay_command: Vec::new(),
    };
    let json = serde_json::to_value(&view).expect("the view serialises");
    assert!(
        json.get("reason").is_none(),
        "an absent diagnostic crosses as no key, never a null the web must read",
    );
    assert_eq!(json["status"], "off");
    let listening = McpStatusView {
        status: McpStateWord::Listening,
        ..view
    };
    assert_eq!(
        serde_json::to_value(&listening).expect("the view serialises")["status"],
        "listening",
    );
}
