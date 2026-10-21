// SPDX-License-Identifier: GPL-3.0-only
//! The binary smoke: spawn the real echo process, hand it an init, and read
//! its ready. This is the only test that crosses the actual process boundary;
//! everything else drives the loop in-process.

use std::io::{Read as _, Write as _};

#[test]
fn the_real_binary_answers_an_init_with_its_ready() {
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_astra-plugin-echo"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the echo binary spawns");

    let init = serde_json::json!({
        "type": "plugin_init",
        "manifest": {
            "id": "dev.astra.echo",
            "name": "Echo",
            "version": "0.1.0",
            "api_level": 1,
            "aom_version": 1,
            "methods": ["plugin.dev.astra.echo.ping"],
            "permissions": [],
        },
        "grants": [],
        "api_level": 1,
        "aom_version": 1,
        "data_dir": "C:/plugins/echo",
    });
    let mut line = serde_json::to_string(&init).expect("serialises");
    line.push('\n');
    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(line.as_bytes())
        .expect("init written");

    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut buffer = String::new();
    let mut byte = [0u8; 1];
    while !buffer.ends_with('\n') {
        let read = stdout.read(&mut byte).expect("readable");
        assert!(read > 0, "the echo answered before EOF");
        buffer.push(byte[0] as char);
    }
    let ready: serde_json::Value = serde_json::from_str(buffer.trim()).expect("valid JSON");
    assert_eq!(ready["type"], "plugin_ready");
    assert_eq!(ready["api_level"], 1);
    assert_eq!(ready["aom_version"], 1);

    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(b"{\"type\":\"plugin_shutdown\"}\n")
        .expect("shutdown written");
    let status = child.wait().expect("waited");
    assert!(status.success(), "the echo exits cleanly on shutdown");
}
