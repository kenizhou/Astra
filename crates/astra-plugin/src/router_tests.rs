// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the router's three gates: a call must be manifest-declared, its
//! permission granted, and every handle it carries in scope, in that order.
//! The gates are the fail-closed core of the platform: a call that fails any
//! of them never reaches the worker or the host's intents.

use super::*;

use crate::PluginId;

use serde_json::json;

fn manifest(extra_methods: &[&str], permissions: &[&str]) -> PluginManifest {
    let methods = extra_methods
        .iter()
        .map(|m| format!("\"{m}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let permissions = permissions
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mode = if extra_methods.contains(&"before_send") {
        "before_send_mode = \"block\"\n"
    } else {
        ""
    };
    PluginManifest::parse(&format!(
        r#"
id = "dev.astra.echo"
name = "Echo"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = [{methods}]
permissions = [{permissions}]
{mode}"#
    ))
    .expect("test manifest parses")
}

fn table_with_message() -> (HandleTable, ObjectRef) {
    let mut table = HandleTable::new(PluginId::new("dev.astra.echo".to_string()), 1);
    let handle = table.mint(
        HandleKind::Message,
        "work".to_string(),
        "imap:v1:u1".to_string(),
    );
    (table, handle)
}

#[test]
fn a_declared_granted_scoped_call_routes() {
    let router = Router::new(
        manifest(&["message.markRead"], &["read_mailbox"]),
        &[Permission::ReadMailbox],
        true,
    );
    let (table, handle) = table_with_message();
    let handle_arg = Some(handle.clone());
    let args = json!([true]);
    let call = router
        .route(&table, &handle_arg, "message.markRead", &args)
        .expect("routes");
    assert_eq!(call.def.expect("registry method").name, "message.markRead");
    assert_eq!(
        call.handle.expect("handle resolved").key,
        "imap:v1:u1",
        "the router hands the host the internal key, never the plugin"
    );
}

#[test]
fn an_undeclared_method_is_refused_before_any_other_gate() {
    let router = Router::new(
        manifest(&["plugin.dev.astra.echo.ping"], &[]),
        &[Permission::ReadMailbox],
        true,
    );
    let (table, handle) = table_with_message();
    // Granted and in scope, but never declared: the manifest gate fires first.
    let error = router
        .route(&table, &Some(handle), "message.markRead", &json!([true]))
        .expect_err("undeclared");
    assert!(matches!(error, RouteError::UndeclaredMethod(m) if m == "message.markRead"));
}

#[test]
fn an_unregistered_non_custom_method_is_refused() {
    // Cannot happen from a parsed manifest (parse rejects unknown names), but
    // the router checks again: the manifest in hand is the one parsed at
    // install, and a call arrives at runtime.
    let router = Router::new(manifest(&["plugin.dev.astra.echo.ping"], &[]), &[], true);
    let (table, _) = table_with_message();
    let error = router
        .route(&table, &None, "message.exfiltrate", &json!([]))
        .expect_err("unknown");
    assert!(matches!(error, RouteError::UndeclaredMethod(_)));
}

#[test]
fn a_custom_method_routes_without_a_registry_permission() {
    let router = Router::new(manifest(&["plugin.dev.astra.echo.ping"], &[]), &[], true);
    let (table, _) = table_with_message();
    let args = json!({ "n": 1 });
    let call = router
        .route(&table, &None, "plugin.dev.astra.echo.ping", &args)
        .expect("custom routes");
    assert!(call.def.is_none(), "custom methods have no registry entry");
    assert_eq!(call.method, "plugin.dev.astra.echo.ping");
}

#[test]
fn an_ungranted_permission_is_refused() {
    let router = Router::new(
        manifest(&["message.markRead"], &["read_mailbox"]),
        &[], // declared, never granted
        true,
    );
    let (table, handle) = table_with_message();
    let error = router
        .route(&table, &Some(handle), "message.markRead", &json!([true]))
        .expect_err("ungranted");
    assert!(matches!(
        error,
        RouteError::PermissionDenied {
            permission: Permission::ReadMailbox,
            ..
        }
    ));
}

#[test]
fn a_handle_of_the_wrong_kind_is_out_of_scope() {
    let router = Router::new(
        manifest(&["message.markRead"], &["read_mailbox"]),
        &[Permission::ReadMailbox],
        true,
    );
    let mut table = HandleTable::new(PluginId::new("dev.astra.echo".to_string()), 1);
    let folder = table.mint(HandleKind::Folder, "work".to_string(), "inbox".to_string());
    let error = router
        .route(&table, &Some(folder), "message.markRead", &json!([true]))
        .expect_err("wrong kind");
    assert!(matches!(
        error,
        RouteError::OutOfScope {
            expected: HandleKind::Message,
            ..
        }
    ));
}

#[test]
fn stale_foreign_and_unknown_handles_are_refused_with_the_ref_error() {
    let router = Router::new(
        manifest(&["message.markRead"], &["read_mailbox"]),
        &[Permission::ReadMailbox],
        true,
    );
    let (table, _) = table_with_message();
    for raw in [
        "msg.dev.astra.echo.9.1",
        "msg.dev.astra.other.1.1",
        "imap:v1:u1",
    ] {
        let handle = ObjectRef::new(raw.to_string());
        let error = router
            .route(&table, &Some(handle), "message.markRead", &json!([true]))
            .expect_err("ref must be refused");
        assert!(
            matches!(error, RouteError::Ref(_)),
            "raw {raw:?} must surface as a ref error"
        );
    }
}

#[test]
fn a_missing_handle_is_refused_when_the_method_expects_one() {
    let router = Router::new(
        manifest(&["message.markRead"], &["read_mailbox"]),
        &[Permission::ReadMailbox],
        true,
    );
    let (table, _) = table_with_message();
    let error = router
        .route(&table, &None, "message.markRead", &json!([true]))
        .expect_err("missing handle");
    assert!(matches!(error, RouteError::MissingHandle { .. }));
}

#[test]
fn a_handle_is_refused_when_the_method_expects_none() {
    let router = Router::new(
        manifest(&["before_send"], &["send"]),
        &[Permission::Send],
        true,
    );
    let (table, handle) = table_with_message();
    let error = router
        .route(&table, &Some(handle), "before_send", &json!([]))
        .expect_err("unexpected handle");
    assert!(matches!(error, RouteError::UnexpectedHandle { .. }));
}

#[test]
fn a_proposed_method_is_dev_build_only() {
    let table = HandleTable::new(PluginId::new("dev.astra.echo".to_string()), 1);
    let released = Router::new(
        manifest(&["composer.transform"], &["compose_edit"]),
        &[Permission::ComposeEdit],
        false,
    );
    let error = released
        .route(&table, &None, "composer.transform", &json!({}))
        .expect_err("proposed refused in a release build");
    assert!(matches!(error, RouteError::ProposedMethod(m) if m == "composer.transform"));

    let dev = Router::new(
        manifest(&["composer.transform"], &["compose_edit"]),
        &[Permission::ComposeEdit],
        true,
    );
    assert!(
        dev.route(&table, &None, "composer.transform", &json!({}))
            .is_ok(),
        "proposed routes in a dev build"
    );
}

#[test]
fn the_session_lookups_take_an_account_handle() {
    let router = Router::new(
        manifest(&["session.getItemFromID"], &["read_mailbox"]),
        &[Permission::ReadMailbox],
        true,
    );
    let mut table = HandleTable::new(PluginId::new("dev.astra.echo".to_string()), 1);
    let account = table.mint(HandleKind::Account, "work".to_string(), "work".to_string());
    assert!(
        router
            .route(
                &table,
                &Some(account),
                "session.getItemFromID",
                &json!(["<m1@b.test>"])
            )
            .is_ok(),
        "an account handle scopes the lookup"
    );
}
