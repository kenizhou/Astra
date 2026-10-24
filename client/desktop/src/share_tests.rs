// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the share-target entry's host leg: the door's filter
//! (plain files cross, executables and unclean names drop, whatever
//! the hand-off's case or shape) and the launch hold's drain-once
//! state machine.

use std::path::PathBuf;

use super::{drain, hold_files, safe_name, shared_files, SharedFile};

/// A scratch directory of this test's own, wiped so one test's files
/// never reach another's.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-share-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the scratch directory creates");
    dir
}

/// One plain file standing in the scratch directory.
fn plain(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"shared").expect("the file writes");
    path
}

#[test]
fn the_filter_keeps_plain_files_and_drops_everything_unsafe() {
    let dir = scratch("filter");

    let photo = plain(&dir, "photo.png");
    let notes = plain(&dir, "notes.txt");
    // The unsafe type: an extension the OS executes.
    let tool = plain(&dir, "tool.exe");
    // A directory, not a file.
    let folder = dir.join("folder");
    std::fs::create_dir_all(&folder).expect("the folder creates");
    // A path that names nothing.
    let missing = dir.join("missing.pdf");

    let shared = shared_files([photo.clone(), notes, tool, folder, missing]);

    assert_eq!(
        shared,
        vec![
            SharedFile {
                name: "photo.png".to_owned(),
                path: photo.display().to_string(),
            },
            SharedFile {
                name: "notes.txt".to_owned(),
                path: dir.join("notes.txt").display().to_string(),
            },
        ],
        "the plain files cross with name and path; nothing else does"
    );
}

#[test]
fn a_name_that_is_not_plain_never_crosses() {
    // The name check is the cross-platform defence: a control
    // character inside a name is a file Windows itself cannot hold,
    // so the check is pinned on the name, not on a file the OS would
    // refuse to create.
    assert!(safe_name("photo.png"));
    assert!(safe_name("LICENSE"));
    assert!(!safe_name("bad\nname.txt"), "a control character drops");
    assert!(!safe_name("bad\tname.txt"), "a tab drops");
    assert!(!safe_name(""), "an empty name drops");
    assert!(!safe_name("."), "the dot drops");
    assert!(!safe_name(".."), "the dot dot drops");
}

#[test]
fn the_executable_extension_drops_whatever_its_case() {
    let dir = scratch("case");
    let tool = plain(&dir, "TOOL.Exe");
    let script = plain(&dir, "install.BAT");

    assert!(
        shared_files([tool, script]).is_empty(),
        "the type filter reads the extension's case-insensitively"
    );
}

#[test]
fn the_script_host_extensions_drop_like_any_executable() {
    // A stock install executes these on a double-click (WScript, mshta,
    // shortcuts, registry merges), so the door drops them exactly like
    // an executable: the phishing shape is the script, not just the exe.
    let dir = scratch("scripts");
    let dropped = [
        "invoice.js",
        "readme.jse",
        "payload.vbe",
        "sheet.wsf",
        "doc.wsh",
        "patch.hta",
        "link.lnk",
        "old.pif",
        "shell.scf",
        "panel.cpl",
        "merge.reg",
        "app.jar",
    ];
    let paths: Vec<_> = dropped.iter().map(|name| plain(&dir, name)).collect();
    assert!(
        shared_files(paths).is_empty(),
        "every stock script extension drops at the door"
    );
}

#[test]
fn an_extensionless_file_with_a_safe_name_crosses() {
    let dir = scratch("no-extension");
    let license = plain(&dir, "LICENSE");

    let shared = shared_files([license.clone()]);
    assert_eq!(
        shared,
        vec![SharedFile {
            name: "LICENSE".to_owned(),
            path: license.display().to_string(),
        }],
        "a file with no extension is not an executable"
    );
}

#[test]
fn the_hold_drains_once_and_an_empty_handoff_holds_nothing() {
    let app = crate::commands_fakes::FakeApp::new();
    let state = crate::commands_fakes::state(
        "share-hold",
        app,
        crate::commands_fakes::FakeStore::accepting(),
    );

    let files = vec![SharedFile {
        name: "photo.png".to_owned(),
        path: "C:\\somewhere\\photo.png".to_owned(),
    }];
    hold_files(&state, files.clone());
    assert_eq!(
        drain(&state),
        Some(files),
        "the held files drain in the shape they crossed"
    );
    assert_eq!(
        drain(&state),
        None,
        "the hold drains once: a later load holds nothing"
    );

    // An empty hand-off holds nothing, so a drain after one answers
    // nothing.
    hold_files(&state, Vec::new());
    assert_eq!(drain(&state), None);
}
