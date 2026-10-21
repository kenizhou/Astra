// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the host-issued handles: the table mints, the table resolves, and
//! anything the host did not mint is refused with a distinct typed reason.

use super::*;

fn table() -> HandleTable {
    HandleTable::new(PluginId::new("dev.astra.echo".to_string()), 3)
}

#[test]
fn a_minted_handle_resolves_back_to_its_entry() {
    let mut table = table();
    let handle = table.mint(
        HandleKind::Message,
        "work".to_string(),
        "imap:v1:u1".to_string(),
    );
    let entry = table.resolve(&handle).expect("minted handle resolves");
    assert_eq!(entry.kind, HandleKind::Message);
    assert_eq!(entry.account, "work");
    assert_eq!(entry.key, "imap:v1:u1");
}

#[test]
fn the_wire_shape_names_kind_plugin_and_generation() {
    let mut table = table();
    let handle = table.mint(HandleKind::Folder, "work".to_string(), "inbox".to_string());
    let raw = handle.as_str();
    assert!(
        raw.starts_with("fld.dev.astra.echo.3."),
        "handle {raw} must name its kind, plugin, and generation"
    );
}

#[test]
fn minted_handles_never_repeat() {
    let mut table = table();
    let first = table.mint(HandleKind::Message, "work".to_string(), "k1".to_string());
    let second = table.mint(HandleKind::Message, "work".to_string(), "k1".to_string());
    assert_ne!(first, second, "same key must mint distinct handles");
}

#[test]
fn a_client_composed_key_is_not_a_handle() {
    let table = table();
    let forged = ObjectRef::new("msg.dev.astra.echo.3.imap:v1:u1".to_string());
    assert!(matches!(
        table.resolve(&forged),
        Err(RefError::UnknownHandle(_))
    ));
}

#[test]
fn a_handle_from_another_generation_is_stale() {
    let table = table();
    let stale = ObjectRef::new("msg.dev.astra.echo.2.7".to_string());
    assert!(matches!(
        table.resolve(&stale),
        Err(RefError::StaleHandle { .. })
    ));
}

#[test]
fn a_handle_from_another_plugin_is_foreign() {
    let table = table();
    let foreign = ObjectRef::new("msg.dev.astra.other.3.7".to_string());
    assert!(matches!(
        table.resolve(&foreign),
        Err(RefError::ForeignPlugin { .. })
    ));
}

#[test]
fn a_malformed_handle_is_unknown_not_a_panic() {
    let table = table();
    for raw in [
        "",
        "msg",
        "msg.dev.astra.echo",
        "???",
        "msg.dev.astra.echo.x.1",
    ] {
        let handle = ObjectRef::new(raw.to_string());
        assert!(
            matches!(table.resolve(&handle), Err(RefError::UnknownHandle(_))),
            "raw {raw:?} must be an unknown handle"
        );
    }
}

#[test]
fn handle_kinds_round_trip_their_wire_names() {
    for (kind, name) in [
        (HandleKind::Message, "msg"),
        (HandleKind::Folder, "fld"),
        (HandleKind::Account, "acc"),
        (HandleKind::Draft, "dft"),
    ] {
        assert_eq!(kind.wire_name(), name);
        assert_eq!(HandleKind::from_wire_name(name), Some(kind));
    }
    assert_eq!(HandleKind::from_wire_name("nope"), None);
}
