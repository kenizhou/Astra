// SPDX-License-Identifier: GPL-3.0-only
//! The draft-2020-12 input schemas of the tool catalogue, as literals.
//!
//! Every schema is a closed object (`additionalProperties: false`) so a
//! typo'd argument is refused loudly rather than silently dropped, and the
//! serde input types in the module root must agree with them: the tests pin
//! that a value the schema accepts deserialises and a value it rejects does
//! not, so the schema a client reads and the deserializer that parses what
//! it sends cannot drift.

use serde_json::{json, Value};

/// The schema of a tool that takes nothing.
pub(super) fn no_args_schema() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

/// The schema of a tool that names one account.
pub(super) fn account_only_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"account": account_property("The account id, from `list_accounts`.")},
        "required": ["account"],
        "additionalProperties": false,
    })
}

/// The schema of `list_messages`.
pub(super) fn list_messages_schema() -> Value {
    let mut properties = message_properties();
    properties.insert(
        "folder".to_owned(),
        json!({
            "type": "string",
            "description": "The folder key, from `list_folders`. Omit to list the whole mailbox.",
        }),
    );
    properties.insert(
        "unread_only".to_owned(),
        json!({
            "type": "boolean", "description": "Only unread messages.", "default": false,
        }),
    );
    properties.insert("offset".to_owned(), offset_property());
    properties.insert("limit".to_owned(), limit_property());
    json!({
        "type": "object",
        "properties": properties,
        "required": ["account"],
        "additionalProperties": false,
    })
}

/// The schema of `search_messages`.
pub(super) fn search_schema() -> Value {
    let mut properties = message_properties();
    properties.insert(
        "query".to_owned(),
        json!({
            "type": "string", "description": "What to search for.",
        }),
    );
    properties.insert(
        "account".to_owned(),
        account_property("Narrow to one account. Omit to search every exposed account."),
    );
    properties.insert(
        "folder".to_owned(),
        json!({
            "type": "string", "description": "Narrow to one folder. Requires `account`.",
        }),
    );
    properties.insert("offset".to_owned(), offset_property());
    properties.insert("limit".to_owned(), limit_property());
    json!({
        "type": "object",
        "properties": properties,
        "required": ["query"],
        "additionalProperties": false,
    })
}

/// The schema of the per-message tools: `account` and `key` required
/// together.
pub(super) fn message_schema() -> Value {
    json!({
        "type": "object",
        "properties": message_properties(),
        "required": ["account", "key"],
        "additionalProperties": false,
    })
}

/// The schema of `mark_read`.
pub(super) fn mark_read_schema() -> Value {
    let mut properties = message_properties();
    properties.insert(
        "read".to_owned(),
        json!({
            "type": "boolean", "description": "`true` to mark it read, `false` for unread.",
        }),
    );
    json!({
        "type": "object",
        "properties": properties,
        "required": ["account", "key", "read"],
        "additionalProperties": false,
    })
}

/// The schema of `set_flagged`.
pub(super) fn set_flagged_schema() -> Value {
    let mut properties = message_properties();
    properties.insert(
        "flagged".to_owned(),
        json!({
            "type": "boolean", "description": "`true` to flag it, `false` to clear the flag.",
        }),
    );
    json!({
        "type": "object",
        "properties": properties,
        "required": ["account", "key", "flagged"],
        "additionalProperties": false,
    })
}

/// The schema of `create_draft`.
pub(super) fn draft_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "account": account_property("The account to draft from. Omit to use the app's default."),
            "to": {"type": "array", "items": {"type": "string"},
                   "description": "The `To` recipients."},
            "cc": {"type": "array", "items": {"type": "string"},
                   "description": "The `Cc` recipients.", "default": []},
            "bcc": {"type": "array", "items": {"type": "string"},
                    "description": "The `Bcc` recipients.", "default": []},
            "subject": {"type": "string", "description": "The subject."},
            "body_text": {"type": "string", "description": "The body, as plain text."},
            "reply_to": {
                "type": "object",
                "description": "The message this draft replies to, so the draft threads.",
                "properties": message_properties(),
                "required": ["account", "key"],
                "additionalProperties": false,
            },
        },
        "required": ["to", "subject", "body_text"],
        "additionalProperties": false,
    })
}

/// The `account` property with its `description`.
fn account_property(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

/// The per-message identity properties, the pair every per-message schema
/// shares.
fn message_properties() -> serde_json::Map<String, Value> {
    let mut properties = serde_json::Map::new();
    properties.insert(
        "account".to_owned(),
        account_property("The account the message belongs to."),
    );
    properties.insert(
        "key".to_owned(),
        json!({
            "type": "string", "description": "The message's key, from a list or search result.",
        }),
    );
    properties
}

/// The `offset` property every paged tool shares.
fn offset_property() -> Value {
    json!({
        "type": "integer", "description": "How many results to skip, for paging.",
        "minimum": 0, "default": 0,
    })
}

/// The `limit` property every paged tool shares: the same bound the query
/// layer clamps to.
fn limit_property() -> Value {
    json!({
        "type": "integer", "description": "How many results to return.",
        "minimum": 1, "maximum": 50, "default": 20,
    })
}
