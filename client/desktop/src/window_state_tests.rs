// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the window state: the round trip through the app data
//! dir's JSON, and every corrupt shape (a missing file, an
//! unparseable body, an insane geometry) falling back to no state,
//! which leaves the window at the config defaults.

use std::path::PathBuf;

use super::{load, save, WindowState, STATE_FILE};

/// A scratch directory of this test's own, wiped so one test's state
/// file never reaches another's.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-window-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the scratch directory creates");
    dir
}

/// One state of the shape a real window leaves behind.
fn state(width: u32, height: u32, x: i32, y: i32, maximised: bool) -> WindowState {
    WindowState {
        width,
        height,
        x,
        y,
        maximised,
    }
}

#[test]
fn a_frame_above_the_floor_round_trips() {
    // A maximised frame on a small or scaled display can sit under the
    // config minimums, so the floor is what no real frame falls under.
    let dir = scratch("floor");
    let small = state(250, 210, 10, 10, false);
    save(&dir, &small);
    assert_eq!(load(&dir), Some(small));
}

#[test]
fn the_window_state_round_trips() {
    let dir = scratch("round-trip");
    let saved = state(1440, 900, 40, 20, true);

    save(&dir, &saved);

    assert_eq!(
        load(&dir),
        Some(saved),
        "what the close saved is what the next launch restores"
    );
}

#[test]
fn an_unmaximised_state_round_trips_too() {
    let dir = scratch("round-trip-plain");
    // The config's own fixed size (1200 by 800), the geometry a plain
    // launch leaves behind.
    let saved = state(1200, 800, 0, 0, false);

    save(&dir, &saved);

    assert_eq!(load(&dir), Some(saved));
}

#[test]
fn a_missing_file_answers_no_state() {
    let dir = scratch("missing");
    assert_eq!(
        load(&dir),
        None,
        "a first launch stands at the config defaults"
    );
}

#[test]
fn a_corrupt_file_falls_back_to_no_state() {
    let dir = scratch("corrupt");
    std::fs::write(dir.join(STATE_FILE), "{not json at all").expect("the corrupt file writes");

    assert_eq!(
        load(&dir),
        None,
        "an unparseable body is no state, and the config defaults stand"
    );
}

#[test]
fn an_insane_geometry_falls_back_like_a_corrupt_file() {
    let dir = scratch("insane");
    let insane = [
        state(0, 800, 0, 0, false),
        state(1200, 0, 0, 0, false),
        state(199, 800, 0, 0, false),
        state(1200, 199, 0, 0, false),
        state(1200, 800, 200_001, 0, false),
        state(1200, 800, 0, -200_001, false),
    ];
    for saved in insane {
        save(&dir, &saved);
        assert_eq!(
            load(&dir),
            None,
            "a window nobody could size or place is a corrupt file: {saved:?}"
        );
    }
}
