// SPDX-License-Identifier: GPL-3.0-only
//! The gated end-to-end proofs over the live harness: the built relay
//! bridging stdio to a real named pipe, the real astra-mcp server behind
//! the pipe, and behind the server a REAL App, the two-account dial the
//! live suites use (the product's own certificate-acceptance walk) wrapped
//! in the MailBackend port the server serves. Nothing is faked: the corpus
//! is the seeded harness mail, the search rides the engine's real search,
//! and a write through the wire lands in the app's own list.
//!
//! Why this file lives in the relay's crate: only the package that owns a
//! binary target sees `CARGO_BIN_EXE_` for it, so the proof that spawns
//! the relay belongs beside the relay's own tests, and the dial it needs
//! is restated here because astra-app's live harness helpers are
//! crate-private test code. Dev dependencies only; the shipped relay still
//! links nothing of the mail stack.
//!
//! The proofs skip unless `ASTRA_LIVE_HARNESS=1`. Against a running
//! harness, one at a time like every live suite (the dial and the corpus
//! are shared state):
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! cargo test -p astra-mcp-relay -- --nocapture --test-threads=1
//! ```

#![cfg(windows)]

mod fixtures;

use astra_app::{FolderRef, Intent};
use astra_mcp::server::PROTOCOL_VERSION;
use serde_json::json;

use fixtures::{
    both_accounts_app, call_for_account, harness, inbox_key, mark_read, pipe_name, row_by_subject,
    serve_on, server_over, subjects, WireClient, SEEDED,
};

/// The full round trip over real mail through the relay: the handshake, the
/// catalogue (with the send cut visible as an absence), the folders and the
/// corpus listing, the fenced body of one seeded note (and the row it did
/// not mark read), the engine's real search through the gate's set form, a
/// write whose effect shows in the app's own list and survives a refresh,
/// and the restore that leaves the corpus as it was found.
#[tokio::test]
async fn a_stdio_client_reads_and_writes_real_mail_through_the_relay() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, first, _second) = both_accounts_app(&host, &port, &user, &password).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let pane_inbox = inbox_key(&app, &first);
    let inbox = FolderRef::from_parts(first.as_str(), pane_inbox.clone())
        .expect("the INBOX folder reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();

    // A known state for the note the proof reads and writes: unread, the
    // write reaching the server on the same pass.
    mark_read(&app, "Harness note two", false).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let endpoint = pipe_name("round-trip");
    let server = server_over(&app, std::slice::from_ref(&first));
    serve_on(&endpoint, server);
    let mut client = WireClient::spawn(&endpoint);

    let hello = client
        .request(
            1,
            "initialize",
            json!({"protocolVersion": PROTOCOL_VERSION}),
        )
        .await;
    assert_eq!(
        hello["result"]["serverInfo"]["name"], "astra-mail",
        "{hello}"
    );
    client.initialized().await;

    // The catalogue: ten tools, and the recorded cuts visible as absences
    // on the live wire.
    let tools = client.request(2, "tools/list", json!({})).await;
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("the tools answer a list")
        .iter()
        .map(|tool| tool["name"].as_str().expect("every tool is named"))
        .collect();
    assert_eq!(names.len(), 10, "the ten-tool catalogue: {tools}");
    assert!(
        !names.contains(&"send_message"),
        "no send verb exists on this wire, the recorded cut: {tools}"
    );
    assert!(
        !names.contains(&"mark_as_spam"),
        "no spam verb exists on this wire, the recorded cut: {tools}"
    );

    // The folders an agent names are the folders the pane names: the wire's
    // inbox key is the pane's own.
    let folders = call_for_account(&mut client, 3, "list_folders", first.as_str()).await;
    let wire_inbox = folders["result"]["structuredContent"]["folders"]
        .as_array()
        .expect("the folders answer a list")
        .iter()
        .find(|folder| folder["role"] == json!("inbox"))
        .expect("the account lists its inbox")["key"]
        .as_str()
        .expect("the folder key")
        .to_owned();
    assert_eq!(
        wire_inbox, pane_inbox,
        "an agent and a person name the same folder the same way"
    );

    // The corpus listing over the harness mail: the three seeded notes,
    // newest first, and no body on any row.
    let listed = client
        .request(
            4,
            "tools/call",
            json!({"name": "list_messages", "arguments": {
                "account": first.as_str(),
                "folder": wire_inbox,
            }}),
        )
        .await;
    let rows = listed["result"]["structuredContent"]["rows"]
        .as_array()
        .expect("the listing answers rows")
        .to_owned();
    assert_eq!(
        subjects(&listed["result"]["structuredContent"]),
        ["Harness note three", "Harness note two", "Harness note one"],
        "the whole INBOX is the three seeded notes, newest first: {listed}"
    );
    for row in &rows {
        assert!(
            !row.as_object()
                .expect("a row is an object")
                .contains_key("body"),
            "no listing row carries a body: {row}"
        );
    }
    let note_two = rows[1]["key"].as_str().expect("the row's key").to_owned();

    // The fenced body: one seeded note's own words, plain text, inside the
    // untrusted-content fence with its preamble, and reading it marks
    // nothing read on the server.
    let read = client
        .request(
            5,
            "tools/call",
            json!({"name": "get_message", "arguments": {
                "account": first.as_str(),
                "key": note_two,
            }}),
        )
        .await;
    assert_eq!(read["result"]["isError"], json!(false), "{read}");
    let detail = &read["result"]["structuredContent"];
    assert_eq!(detail["subject"], json!("Harness note two"), "{read}");
    let body = detail["body"].as_str().expect("the fenced body");
    assert!(
        body.starts_with("The following is the content of an email."),
        "the fence opens with its preamble: {body}"
    );
    assert!(
        body.contains("\n<untrusted-message-content>\n"),
        "the body sits inside the fence: {body}"
    );
    assert!(
        body.contains("Second note delivered through the harness submission port."),
        "the body is the note's own plain text: {body}"
    );
    assert!(
        body.ends_with("</untrusted-message-content>"),
        "the fence closes: {body}"
    );
    assert_eq!(
        detail["unread"],
        json!(true),
        "the read did not mark it read: {read}"
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        row_by_subject(&app.mailbox_list(), "Harness note two").unread,
        "a refresh after the read still finds the note unread: the query door writes nothing"
    );

    // The engine's real search through the gate's set form: no account
    // named, so the search covers exactly the exposed account, every folder
    // but its trash. The seeded corpus is found across two folders, newest
    // first; the drafts suite keeps a harness-titled draft on the volume by
    // design, so the pin is the seeded four in their order rather than the
    // whole answer.
    let search = client
        .request(
            6,
            "tools/call",
            json!({"name": "search_messages", "arguments": {"query": "harness"}}),
        )
        .await;
    let found = subjects(&search["result"]["structuredContent"]);
    assert!(
        found
            .iter()
            .all(|subject| subject.to_ascii_lowercase().contains("harness")),
        "every hit matches the query: {search}"
    );
    let seeded: Vec<&str> = found
        .iter()
        .copied()
        .filter(|subject| SEEDED.contains(subject))
        .collect();
    assert_eq!(
        seeded, SEEDED,
        "the search finds every seeded note across two folders, newest first: {search}"
    );

    // The write through the same door the user's click uses: the row flips
    // in the app's own list and the state survives a refresh.
    let written = client
        .request(
            7,
            "tools/call",
            json!({"name": "mark_read", "arguments": {
                "account": first.as_str(),
                "key": note_two,
                "read": true,
            }}),
        )
        .await;
    assert_eq!(
        written["result"]["structuredContent"]["outcome"],
        json!("marked read"),
        "{written}"
    );
    assert!(
        !row_by_subject(&app.mailbox_list(), "Harness note two").unread,
        "the app's own list shows the row read"
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        !row_by_subject(&app.mailbox_list(), "Harness note two").unread,
        "the write reached the server: the read state survives a refresh"
    );

    // The restore: the same call the other way round, so the corpus keeps
    // its seeded shape for the suites that pin it.
    let restored = client
        .request(
            8,
            "tools/call",
            json!({"name": "mark_read", "arguments": {
                "account": first.as_str(),
                "key": note_two,
                "read": false,
            }}),
        )
        .await;
    assert_eq!(
        restored["result"]["structuredContent"]["outcome"],
        json!("marked unread"),
        "{restored}"
    );
    assert!(
        row_by_subject(&app.mailbox_list(), "Harness note two").unread,
        "the note is unread again"
    );

    client.finish().await;
}

/// The gate end to end: an account the user never exposed is refused
/// through the whole stack, relay, server, and gate alike, and is not even
/// named by `list_accounts`. The refusal is the same for a configured
/// account the user kept and for an id nobody holds, because which other
/// mailboxes exist is itself a disclosure.
#[tokio::test]
async fn an_unlisted_account_is_refused_through_the_whole_stack() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, first, second) = both_accounts_app(&host, &port, &user, &password).await;

    let endpoint = pipe_name("gate");
    let server = server_over(&app, std::slice::from_ref(&first));
    serve_on(&endpoint, server);
    let mut client = WireClient::spawn(&endpoint);

    client
        .request(
            1,
            "initialize",
            json!({"protocolVersion": PROTOCOL_VERSION}),
        )
        .await;

    // The one exposed account is the whole answer: the configured second
    // account is not even named.
    let accounts = client
        .request(
            2,
            "tools/call",
            json!({"name": "list_accounts", "arguments": {}}),
        )
        .await;
    let named = accounts["result"]["structuredContent"]["accounts"]
        .as_array()
        .expect("the accounts answer a list")
        .to_owned();
    assert_eq!(named.len(), 1, "exactly one account is exposed: {accounts}");
    assert_eq!(named[0]["id"], json!(first.as_str()), "{accounts}");
    assert!(
        !accounts.to_string().contains(second.as_str()),
        "an unlisted account is not even named: {accounts}"
    );

    // The refusal for the configured but unexposed account, and the same
    // words for an id nobody holds.
    let held = call_for_account(&mut client, 3, "list_folders", second.as_str()).await;
    assert_eq!(held["result"]["isError"], json!(true), "{held}");
    let held_text = held["result"]["content"][0]["text"]
        .as_str()
        .expect("the refusal says why")
        .to_owned();
    assert!(
        held_text.contains("not available to assistants"),
        "the refusal names the exposure decision: {held_text}"
    );

    let unknown = call_for_account(&mut client, 4, "list_folders", "nobody-holds-this-id").await;
    assert_eq!(unknown["result"]["isError"], json!(true), "{unknown}");
    let unknown_text = unknown["result"]["content"][0]["text"]
        .as_str()
        .expect("the refusal says why")
        .to_owned();
    // Each refusal echoes the id the caller itself sent, and nothing else
    // differs: the sentence cannot say whether the account exists.
    assert_eq!(
        held_text.replace(second.as_str(), "<id>"),
        unknown_text.replace("nobody-holds-this-id", "<id>"),
        "the refusal is the same sentence for a kept-back account and a nonexistent one"
    );

    client.finish().await;
}
