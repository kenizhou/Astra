// SPDX-License-Identifier: GPL-3.0-only
//! Rules store tests: the migration lands the table, and the CRUD round
//! trips with every field intact, the id first among them. The actions
//! column rides as the caller's JSON text: the closed action set is
//! astra-app's vocabulary, pinned in its own tests.

use crate::{AiStore, NewRule, RuleId};

/// A migrated in-memory store.
fn store() -> AiStore {
    AiStore::open_in_memory().expect("in-memory store opens and migrates")
}

/// One insert with every field set to a distinct value so a round trip
/// cannot pass by coincidence.
fn new_rule(id: &str, name: &str) -> NewRule {
    NewRule {
        id: RuleId::new(id.to_string()),
        name: name.to_string(),
        condition: format!("The message is a {name} mail"),
        actions: "[\"mark_read\",\"set_flagged\"]".to_string(),
        enabled: true,
    }
}

#[test]
fn the_rules_migration_lands_the_table() {
    let store = store();
    let conn = store.test_conn();
    let conn = conn.lock().expect("test connection");
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'rules'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master read");
    assert_eq!(found, 1, "the rules table is missing after migrations");
}

#[test]
fn a_rule_round_trips_every_field() {
    let store = store();
    store
        .create_rule(&new_rule("rule-1", "Newsletters"))
        .expect("the rule creates");
    let rules = store.list_rules().expect("the rule lists");
    assert_eq!(rules.len(), 1, "exactly the one rule");
    let rule = &rules[0];
    assert_eq!(rule.id.as_str(), "rule-1");
    assert_eq!(rule.name, "Newsletters");
    assert_eq!(rule.condition, "The message is a Newsletters mail");
    assert_eq!(rule.actions, "[\"mark_read\",\"set_flagged\"]");
    assert!(rule.enabled, "a fresh rule is enabled by default");
    assert!(
        rule.created_at > 0,
        "the store stamps the creation time itself"
    );
}

#[test]
fn listed_rules_keep_a_stable_order() {
    let store = store();
    for name in ["Zebra", "Alpha", "Middle"] {
        store
            .create_rule(&new_rule(&name.to_lowercase(), name))
            .expect("the rule creates");
    }
    let names: Vec<String> = store
        .list_rules()
        .expect("the rules list")
        .into_iter()
        .map(|rule| rule.name)
        .collect();
    assert_eq!(
        names,
        vec![
            "Alpha".to_string(),
            "Middle".to_string(),
            "Zebra".to_string()
        ],
        "same-second creations fall back to name order, so the list is stable",
    );
}

#[test]
fn an_inserted_disabled_rule_stays_disabled() {
    let store = store();
    let mut rule = new_rule("rule-1", "Newsletters");
    rule.enabled = false;
    store.create_rule(&rule).expect("the rule creates");
    let rules = store.list_rules().expect("the rules list");
    assert!(!rules[0].enabled, "the insert keeps the caller's flag");
}

#[test]
fn update_enabled_flips_the_flag_and_reports_an_unknown_id() {
    let store = store();
    store
        .create_rule(&new_rule("rule-1", "Newsletters"))
        .expect("the rule creates");
    assert!(
        store
            .set_rule_enabled(&RuleId::new("rule-1".to_string()), false)
            .expect("the update runs"),
        "the known rule reports changed",
    );
    let rules = store.list_rules().expect("the rules list");
    assert!(!rules[0].enabled, "the flag landed");
    assert!(store
        .set_rule_enabled(&RuleId::new("rule-1".to_string()), true)
        .expect("the update runs"),);
    assert!(
        store.list_rules().expect("the rules list")[0].enabled,
        "the flag landed back",
    );
    assert!(
        !store
            .set_rule_enabled(&RuleId::new("no-such-rule".to_string()), true)
            .expect("the update runs"),
        "an unknown id changes nothing and says so",
    );
}

#[test]
fn delete_removes_the_rule_and_reports_an_unknown_id() {
    let store = store();
    store
        .create_rule(&new_rule("rule-1", "Newsletters"))
        .expect("the rule creates");
    assert!(
        store
            .delete_rule(&RuleId::new("rule-1".to_string()))
            .expect("the delete runs"),
        "the known rule reports deleted",
    );
    assert!(
        store.list_rules().expect("the rules list").is_empty(),
        "the rule is gone",
    );
    assert!(
        !store
            .delete_rule(&RuleId::new("rule-1".to_string()))
            .expect("the delete runs"),
        "a second delete changes nothing and says so",
    );
}

#[test]
fn a_repeated_id_fails_loudly() {
    let store = store();
    store
        .create_rule(&new_rule("rule-1", "Newsletters"))
        .expect("the rule creates");
    let err = store
        .create_rule(&new_rule("rule-1", "Duplicates"))
        .expect_err("the primary key must refuse a repeated id");
    assert!(err.to_string().contains("ai store sql failure"), "{err}");
}

#[test]
fn generated_rule_ids_are_distinct_and_nonempty() {
    let first = RuleId::generate();
    let second = RuleId::generate();
    assert!(!first.as_str().is_empty());
    assert_ne!(first, second);
}
