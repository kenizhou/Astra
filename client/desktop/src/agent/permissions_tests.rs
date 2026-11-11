// SPDX-License-Identifier: GPL-3.0-only
//! The permission bridge's tests: the ask and answer round trips, the
//! audit row's redacted shapes, and the gating map's preview column
//! (docs/cli.md) pinned shape for shape.

use super::*;

fn approval(id: &str, correlation: Option<&str>) -> PendingApproval {
    PendingApproval {
        id: id.to_string(),
        tool_call_id: "call-1".to_string(),
        tool: "save_draft".to_string(),
        level: PermissionLevel::Confirm,
        args: serde_json::json!({ "account": "acct-1" }),
        preview: None,
        correlation: correlation.map(str::to_string),
    }
}

fn bridge() -> PermissionBridge {
    PermissionBridge::new(std::sync::Arc::new(
        AiStore::open_in_memory().expect("the in-memory store opens"),
    ))
}

#[test]
fn register_respond_round_trips_one_shot() {
    let bridge = bridge();
    bridge
        .register(approval("perm-1", Some("corr-1")))
        .expect("registers");
    assert_eq!(bridge.pending().len(), 1);

    let consumed = bridge
        .respond("perm-1", true, false)
        .expect("the answer consumes the ask");
    assert_eq!(consumed.correlation.as_deref(), Some("corr-1"));
    assert_eq!(
        bridge.pending(),
        [] as [crate::agent::permissions::PendingApproval; 0]
    );

    let again = bridge.respond("perm-1", true, false);
    assert!(again.is_err(), "an answer is one-shot");
}

#[test]
fn duplicate_ids_are_refused() {
    let bridge = bridge();
    bridge
        .register(approval("perm-1", None))
        .expect("registers");
    assert!(bridge.register(approval("perm-1", None)).is_err());
    assert_eq!(bridge.pending().len(), 1);
}

/// The audit row a settled save_draft ask wrote, read straight from
/// the ledger file: the store only appends, so the test brings its
/// own connection.
fn recorded_preview(dir: &std::path::Path) -> String {
    let conn =
        rusqlite::Connection::open(dir.join("ai.sqlite")).expect("the ledger opens for reading");
    conn.query_row(
        "SELECT arg_preview FROM audit_log WHERE verb = 'permission save_draft'",
        [],
        |row| row.get(0),
    )
    .expect("the permission row reads back")
}

#[test]
fn the_audit_row_carries_shapes_not_message_text() {
    let dir = super::super::testkit::scratch("permission-preview");
    let bridge = PermissionBridge::new(std::sync::Arc::new(
        AiStore::open(&dir.join("ai.sqlite")).expect("the file store opens"),
    ));
    bridge
        .register(PendingApproval {
            id: "perm-1".to_string(),
            tool_call_id: "call-1".to_string(),
            tool: "save_draft".to_string(),
            level: PermissionLevel::Confirm,
            args: serde_json::json!({
                "account": "acct-1",
                "from": "ada@example.com",
                "to": ["bob@example.com", "carol@example.com"],
                "cc": ["dan@example.com"],
                "subject": "Re: the meridian code is halcyon-dawn",
                "body": "burn after reading: zulu-seven-bravo",
            }),
            preview: None,
            correlation: Some("corr-1".to_string()),
        })
        .expect("registers");
    bridge
        .respond("perm-1", true, false)
        .expect("the answer consumes the ask");

    let preview = recorded_preview(&dir);
    assert_eq!(
        preview, "account=acct-1 from=1 to=2 cc=1 subject_chars=37 body_chars=36",
        "the preview mirrors the CLI's shape vocabulary"
    );
    for planted in ["example.com", "halcyon-dawn", "zulu-seven-bravo"] {
        assert!(
            !preview.contains(planted),
            "the recorded row leaked '{planted}': {preview}"
        );
    }
}

#[test]
fn fail_close_drains_and_restores_survive() {
    let bridge = bridge();
    bridge
        .register(approval("perm-1", Some("corr-1")))
        .expect("registers");
    let drained = bridge.fail_close_all("sidecar died");
    assert_eq!(drained.len(), 1);
    assert_eq!(
        bridge.pending(),
        [] as [crate::agent::permissions::PendingApproval; 0]
    );

    bridge.restore(drained[0].clone());
    assert_eq!(bridge.pending().len(), 1);
}

/// The gating map's preview column (docs/cli.md): every tool the sidecar
/// ships answers with the CLI's redacted vocabulary, the optional fields
/// ride only when the args carry them, and a genuinely unknown tool falls
/// to the generic shape (fail closed).
#[test]
fn the_ten_preview_shapes_mirror_the_cli_vocabulary() {
    let cases: Vec<(&str, serde_json::Value, &str)> = vec![
        (
            "list_folders",
            serde_json::json!({ "account": "acct-1" }),
            "account=acct-1",
        ),
        (
            "list_messages",
            serde_json::json!({ "account": "acct-1", "folder": "inbox", "limit": 25 }),
            "account=acct-1 folder=inbox limit=25",
        ),
        (
            "list_messages",
            serde_json::json!({ "account": "acct-1" }),
            "account=acct-1",
        ),
        (
            "read_message",
            serde_json::json!({ "account": "acct-1", "keys": ["msg-1", "msg-2"] }),
            "account=acct-1 keys=2",
        ),
        (
            "search_messages",
            serde_json::json!({
                "query": "quarterly report",
                "account": "acct-1",
                "folder": "inbox",
                "offset": 20,
            }),
            "query_chars=16 account=acct-1 folder=inbox offset=20",
        ),
        (
            "search_messages",
            serde_json::json!({ "query": "report" }),
            "query_chars=6",
        ),
        (
            "save_draft",
            serde_json::json!({
                "account": "acct-1",
                "from": "ada@example.com",
                "to": ["bob@example.com", "carol@example.com"],
                "cc": ["dan@example.com"],
                "subject": "Re: Friday",
                "body": "See you then.",
            }),
            "account=acct-1 from=1 to=2 cc=1 subject_chars=10 body_chars=13",
        ),
        (
            "discard_draft",
            serde_json::json!({ "account": "acct-1", "key": "draft-9" }),
            "account=acct-1 key=draft-9",
        ),
        (
            "mark_read",
            serde_json::json!({ "account": "acct-1", "key": "msg-1", "read": true }),
            "account=acct-1 key=msg-1 read=true",
        ),
        (
            "mark_read",
            serde_json::json!({ "account": "acct-1", "key": "msg-1", "read": false }),
            "account=acct-1 key=msg-1 read=false",
        ),
        (
            "set_flagged",
            serde_json::json!({ "account": "acct-1", "key": "msg-1", "flagged": true }),
            "account=acct-1 key=msg-1 flagged=true",
        ),
        (
            "set_flagged",
            serde_json::json!({ "account": "acct-1", "key": "msg-1", "flagged": false }),
            "account=acct-1 key=msg-1 flagged=false",
        ),
        (
            "archive_message",
            serde_json::json!({ "account": "acct-1", "key": "msg-1" }),
            "account=acct-1 key=msg-1",
        ),
        (
            "move_to_trash",
            serde_json::json!({ "account": "acct-1", "key": "msg-1" }),
            "account=acct-1 key=msg-1",
        ),
        (
            "send_message",
            serde_json::json!({ "account": "acct-1", "key": "msg-1" }),
            "fields=2",
        ),
    ];
    for (tool, args, want) in cases {
        assert_eq!(
            shaped_arg_preview(tool, &args),
            want,
            "the {tool} preview shape"
        );
    }
}

/// The search preview carries the query's length, never its text: the
/// query is caller-typed mail text, the drafts body's own discipline.
#[test]
fn the_search_preview_never_carries_the_query_text() {
    let args = serde_json::json!({ "query": "from:ada@example.com halcyon-dawn" });
    let preview = shaped_arg_preview("search_messages", &args);
    assert_eq!(preview, "query_chars=33");
    for planted in ["example.com", "halcyon-dawn"] {
        assert!(
            !preview.contains(planted),
            "the preview leaked '{planted}': {preview}"
        );
    }
}
