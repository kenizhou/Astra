// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the [`Lifecycle`] state machine: every legal edge moves the plugin
//! forward, every illegal edge is refused, and [`Lifecycle::Failed`] is terminal.
//!
//! The graph is the host's supervision contract; a transition the host refuses here
//! is a transition no future supervisor code can accidentally allow.

use super::*;

#[test]
fn the_happy_path_walks_discovered_loaded_activated() {
    let state = Lifecycle::Discovered
        .transition(LifecycleEvent::Load)
        .expect("discovered loads")
        .transition(LifecycleEvent::Activate)
        .expect("loaded activates");
    assert_eq!(state, Lifecycle::Activated);
}

#[test]
fn failure_is_legal_from_every_live_state() {
    for state in [
        Lifecycle::Discovered,
        Lifecycle::Loaded,
        Lifecycle::Activated,
    ] {
        let failed = state
            .clone()
            .transition(LifecycleEvent::Fail("load error".to_owned()))
            .unwrap_or_else(|err| panic!("state {state:?} may fail: {err}"));
        assert_eq!(failed, Lifecycle::Failed("load error".to_owned()));
    }
}

#[test]
fn failure_preserves_its_reason() {
    let failed = Lifecycle::Loaded
        .transition(LifecycleEvent::Fail("hook panicked".to_owned()))
        .expect("loaded may fail");
    match failed {
        Lifecycle::Failed(reason) => assert_eq!(reason, "hook panicked"),
        other => panic!("expected failure, got {other:?}"),
    }
}

#[test]
fn activating_a_discovered_plugin_is_rejected() {
    let error = Lifecycle::Discovered
        .transition(LifecycleEvent::Activate)
        .expect_err("must load before activating");
    assert_eq!(error.from, Lifecycle::Discovered);
    assert_eq!(error.event, LifecycleEvent::Activate);
}

#[test]
fn loading_a_loaded_plugin_is_rejected() {
    assert!(Lifecycle::Loaded.transition(LifecycleEvent::Load).is_err());
}

#[test]
fn activating_an_activated_plugin_is_rejected() {
    assert!(Lifecycle::Activated
        .transition(LifecycleEvent::Activate)
        .is_err());
}

#[test]
fn loading_an_activated_plugin_is_rejected() {
    assert!(Lifecycle::Activated
        .transition(LifecycleEvent::Load)
        .is_err());
}

#[test]
fn a_failed_plugin_never_resurrects() {
    let failed = Lifecycle::Failed("terminal".to_owned());
    assert!(failed.clone().transition(LifecycleEvent::Load).is_err());
    assert!(failed.clone().transition(LifecycleEvent::Activate).is_err());
    assert!(failed
        .transition(LifecycleEvent::Fail("again".to_owned()))
        .is_err());
}

#[test]
fn the_rejected_transition_error_names_both_sides() {
    let error = Lifecycle::Discovered
        .transition(LifecycleEvent::Activate)
        .expect_err("illegal edge");
    let rendered = error.to_string();
    assert!(rendered.contains("Discovered"), "{rendered}");
    assert!(rendered.contains("Activate"), "{rendered}");
}
