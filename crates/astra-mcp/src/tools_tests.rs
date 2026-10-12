// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the tool schemas: the catalog's shape (the ten tools, the two
//! recorded cuts pinned as absences), every schema's draft-2020-12
//! discipline, the account/key pairing at the boundary, the schema-to-type
//! agreement that keeps a client from sending exactly what a tool documented
//! and getting a parse error, and the honest annotations. The fence's own
//! tests live in `fence_tests`.

use serde_json::{json, Value};

use crate::tools::{
    self, AccountArgs, Annotations, DraftArgs, ListMessagesArgs, MarkReadArgs, MessageArgs, NoArgs,
    SearchArgs, SetFlaggedArgs,
};

/// The catalog's names, in order: the stable answer to `tools/list`.
#[test]
fn the_catalog_lists_the_ten_tools_in_a_stable_order() {
    let names: Vec<&str> = tools::tools().iter().map(|tool| tool.name).collect();
    assert_eq!(
        names,
        [
            "list_accounts",
            "list_folders",
            "list_messages",
            "search_messages",
            "get_message",
            "mark_read",
            "set_flagged",
            "archive_message",
            "move_to_trash",
            "create_draft",
        ],
    );
}

/// The two cuts the design records, pinned as absences so a later change
/// cannot quietly grow them back.
#[test]
fn no_send_and_no_spam_tool_exists() {
    let names: Vec<&str> = tools::tools().iter().map(|tool| tool.name).collect();
    assert!(
        !names.contains(&"send_message"),
        "send-class verbs are refused to the agent; humans decide and send",
    );
    assert!(
        !names.contains(&"mark_as_spam"),
        "the dispatch carries no spam verb",
    );
}

/// Every tool's schema: a closed draft-2020-12 object with snake_case
/// properties and `required` naming only properties that exist.
#[test]
fn every_schema_is_a_closed_draft_2020_12_object() {
    for tool in tools::tools() {
        let schema = &tool.input_schema;
        assert_eq!(
            schema["type"], "object",
            "{}: the root is an object",
            tool.name
        );
        assert_eq!(
            schema["additionalProperties"], false,
            "{}: unknown arguments are refused, not silently dropped",
            tool.name,
        );
        assert!(
            schema.get("$defs").is_none(),
            "{}: the schema is self-contained",
            tool.name,
        );
        let properties = schema["properties"].as_object().expect("a properties map");
        for (name, property) in properties {
            assert_eq!(
                name,
                &name.to_snake_case(),
                "{}: property {name} is snake_case",
                tool.name,
            );
            assert!(
                property.get("type").is_some(),
                "{}: property {name} carries a type",
                tool.name,
            );
        }
        for required in required_names(schema) {
            assert!(
                properties.contains_key(required),
                "{}: required {required} is a declared property",
                tool.name,
            );
        }
    }
}

/// The wrong-account-routing class stays unrepresentable: every per-message
/// tool requires `account` and `key` together, and the input type carries
/// them in one struct that cannot be built half-apart.
#[test]
fn the_account_and_key_pair_always_travel_together() {
    let per_message = [
        "get_message",
        "mark_read",
        "set_flagged",
        "archive_message",
        "move_to_trash",
    ];
    for tool in tools::tools() {
        let required = required_names(&tool.input_schema);
        let pairs_account = required.contains(&"account");
        let pairs_key = required.contains(&"key");
        assert_eq!(
            pairs_key,
            per_message.contains(&tool.name),
            "{}: exactly the per-message tools name a key",
            tool.name,
        );
        assert!(
            !pairs_key || pairs_account,
            "{}: a key never travels without its account",
            tool.name,
        );
    }
}

/// A value the schema accepts deserialises into the tool's input type, and a
/// value the schema rejects (an unknown argument) fails: the schema a client
/// reads and the deserializer that parses what it sends cannot drift.
#[test]
fn the_schemas_and_the_input_types_agree() {
    let accepted: Vec<(&str, Value)> = vec![
        ("list_accounts", json!({})),
        ("list_folders", json!({"account": "work"})),
        (
            "list_messages",
            json!({"account": "work", "unread_only": true, "offset": 40, "limit": 50}),
        ),
        ("search_messages", json!({"query": "invoice"})),
        ("get_message", json!({"account": "work", "key": "m1"})),
        (
            "mark_read",
            json!({"account": "work", "key": "m1", "read": false}),
        ),
        (
            "set_flagged",
            json!({"account": "work", "key": "m1", "flagged": true}),
        ),
        ("archive_message", json!({"account": "work", "key": "m1"})),
        ("move_to_trash", json!({"account": "work", "key": "m1"})),
        (
            "create_draft",
            json!({"account": "work", "to": ["ada@example.test"], "subject": "Report",
                   "body_text": "Attached.", "reply_to": {"account": "work", "key": "m1"}}),
        ),
    ];
    for (name, value) in accepted {
        let parsed = match name {
            "list_accounts" => parse::<NoArgs>(value),
            "list_folders" => parse::<AccountArgs>(value),
            "list_messages" => parse::<ListMessagesArgs>(value),
            "search_messages" => parse::<SearchArgs>(value),
            "get_message" | "archive_message" | "move_to_trash" => parse::<MessageArgs>(value),
            "mark_read" => parse::<MarkReadArgs>(value),
            "set_flagged" => parse::<SetFlaggedArgs>(value),
            "create_draft" => parse::<DraftArgs>(value),
            other => panic!("no input type mapped for {other}"),
        };
        assert!(
            parsed.is_ok(),
            "{name}: the schema-accepted value parsed: {:?}",
            parsed.err(),
        );
    }

    let rejected: Vec<(&str, Value)> = vec![
        ("list_accounts", json!({"account": "work"})),
        ("list_folders", json!({"account": "work", "key": "m1"})),
        // A listing and a search carry no message key: the input types
        // refuse one, and the schemas must never advertise what the types
        // reject (a client sending the documented shape would fail).
        ("list_messages", json!({"account": "work", "key": "m1"})),
        ("search_messages", json!({"query": "x", "key": "m1"})),
        ("get_message", json!({"account": "work"})),
        (
            "mark_read",
            json!({"account": "work", "key": "m1", "read": true, "unread": false}),
        ),
        (
            "create_draft",
            json!({"account": "work", "to": [], "subject": "S", "body_text": "B",
                   "reply_to": {"account": "work"}}),
        ),
    ];
    for (name, value) in rejected {
        let parsed = match name {
            "list_accounts" => parse::<NoArgs>(value),
            "list_folders" => parse::<AccountArgs>(value),
            "list_messages" => parse::<ListMessagesArgs>(value),
            "search_messages" => parse::<SearchArgs>(value),
            "get_message" => parse::<MessageArgs>(value),
            "mark_read" => parse::<MarkReadArgs>(value),
            "create_draft" => parse::<DraftArgs>(value),
            other => panic!("no input type mapped for {other}"),
        };
        assert!(
            parsed.is_err(),
            "{name}: the schema-rejected value failed to parse",
        );
    }
}

/// Deserialises `value` as `T`, keeping the error text for the failure
/// message.
fn parse<T: serde::de::DeserializeOwned>(value: Value) -> Result<(), String> {
    serde_json::from_value::<T>(value)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

/// The paging bound is pinned on both sides: the schema states it and the
/// query layer clamps to the same numbers.
#[test]
fn the_paging_bound_is_one_to_fifty() {
    for tool in tools::tools() {
        let Some(limit) = tool.input_schema["properties"].get("limit") else {
            continue;
        };
        assert_eq!(limit["minimum"], 1, "{}", tool.name);
        assert_eq!(limit["maximum"], 50, "{}", tool.name);
        assert_eq!(limit["default"], 20, "{}", tool.name);
    }
}

/// The annotations tell the truth: a client shows a permission prompt off
/// these, so marking a destructive tool read-only would remove the one place
/// a human is asked before mail moves.
#[test]
fn the_annotations_are_set_honestly() {
    let expected: Vec<(&str, Annotations)> = vec![
        ("list_accounts", Annotations::READ),
        ("list_folders", Annotations::READ),
        ("list_messages", Annotations::READ),
        ("search_messages", Annotations::READ),
        ("get_message", Annotations::READ),
        ("mark_read", Annotations::IDEMPOTENT),
        ("set_flagged", Annotations::IDEMPOTENT),
        ("archive_message", Annotations::IDEMPOTENT),
        ("move_to_trash", Annotations::DESTRUCTIVE),
        ("create_draft", Annotations::NEUTRAL),
    ];
    for (name, annotations) in expected {
        let tool = tools::tools()
            .into_iter()
            .find(|tool| tool.name == name)
            .expect("the catalog holds the tool");
        assert_eq!(tool.annotations, annotations, "{name}");
    }
}

/// The `required` property names of one schema, in order.
fn required_names(schema: &Value) -> Vec<&str> {
    schema["required"]
        .as_array()
        .map_or_else(Vec::new, |names| {
            names.iter().filter_map(Value::as_str).collect()
        })
}

/// A helper for the snake_case assertion: lowercases and joins on
/// underscores the way the property names are spelled.
trait SnakeCase {
    /// The snake_case spelling of this string.
    fn to_snake_case(&self) -> String;
}

impl SnakeCase for str {
    fn to_snake_case(&self) -> String {
        let mut out = String::new();
        for (index, ch) in self.chars().enumerate() {
            if ch.is_ascii_uppercase() {
                if index > 0 {
                    out.push('_');
                }
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(ch);
            }
        }
        out
    }
}

#[test]
fn the_listing_and_search_schemas_advertise_no_key() {
    for name in ["list_messages", "search_messages"] {
        let tool = tools::tools()
            .into_iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("the {name} tool stands in the catalog"));
        let properties = tool.input_schema["properties"].as_object().unwrap();
        assert!(
            !properties.contains_key("key"),
            "{name}: a listing carries no message key, so the schema must not offer one"
        );
    }
}
