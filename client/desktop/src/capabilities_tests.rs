// SPDX-License-Identifier: GPL-3.0-only
//! The capability manifests' shape test: the message window family's entry
//! mirrors the composer's under its own wildcard, so every window the
//! reading batch opens (the pop-out message window, the print surface over
//! one message) carries the window controls a frameless webview needs and
//! nothing wider. The files are the ones the tauri build embeds, read from
//! the working tree the gate walks.

use std::{fs, path::PathBuf};

/// One capability entry, parsed: the manifest under `capabilities/` the
/// tauri build reads at compile time.
fn entry(name: &str) -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("capabilities")
        .join(format!("{name}.json"));
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("the {name} capability entry reads: {err}"));
    serde_json::from_str(&raw).expect("the entry parses as JSON")
}

#[test]
fn the_message_capability_mirrors_the_composer_entry_under_its_own_wildcard() {
    let composer = entry("composer");
    let message = entry("message");

    assert_eq!(message["identifier"], "message", "its own identifier");
    assert_eq!(
        message["windows"],
        serde_json::json!(["message-*"]),
        "the message family's wildcard: the reading window and the print \
         surface both label themselves under it"
    );
    assert_eq!(
        message["permissions"], composer["permissions"],
        "the composer's own permission set, control for control"
    );
}
