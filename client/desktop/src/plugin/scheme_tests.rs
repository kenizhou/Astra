// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the frame's asset scheme: traversal and cross-plugin reads are
//! refused, a missing file is a 404, and an HTML answer carries the frame's
//! CSP.

use super::*;

use crate::plugin::testkit;

/// A registry whose one plugin's directory holds a page and a script.
fn registry_with_assets(name: &str) -> PluginRegistry {
    let root = testkit::scratch(name);
    let dir = root.join("dev.astra.keys");
    std::fs::create_dir_all(dir.join("assets")).expect("assets dir");
    // Discovery needs a parseable manifest before the directory answers.
    std::fs::write(
        dir.join("manifest.toml"),
        r#"
id = "dev.astra.keys"
name = "Keys"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = ["plugin.dev.astra.keys.ping"]
permissions = []
"#,
    )
    .expect("manifest");
    std::fs::write(dir.join("keys.html"), "<p>keys</p>").expect("page");
    std::fs::write(dir.join("assets/app.js"), "console.log(1)").expect("script");
    let registry = PluginRegistry::new(root, vec![], true);
    registry.list();
    registry
}

#[test]
fn a_page_answers_with_its_bytes_and_the_frame_csp() {
    let registry = registry_with_assets("scheme-page");
    let answer = answer_asset(&registry, "astra-plugin://dev.astra.keys/keys.html");
    assert_eq!(answer.status(), 200);
    assert_eq!(answer.body(), b"<p>keys</p>");
    assert_eq!(
        answer.headers().get("content-type").expect("the mime"),
        "text/html"
    );
    let csp = answer
        .headers()
        .get("content-security-policy")
        .expect("the CSP")
        .to_str()
        .expect("utf8")
        .to_owned();
    assert!(csp.contains("default-src 'none'"), "{csp}");
    assert!(csp.contains("script-src 'self'"), "{csp}");
}

#[test]
fn a_nested_asset_answers_with_its_mime() {
    let registry = registry_with_assets("scheme-js");
    let answer = answer_asset(&registry, "astra-plugin://dev.astra.keys/assets/app.js");
    assert_eq!(answer.status(), 200);
    assert_eq!(
        answer.headers().get("content-type").expect("the mime"),
        "text/javascript"
    );
    // Only HTML carries the frame's CSP.
    assert!(answer.headers().get("content-security-policy").is_none());
}

#[test]
fn traversal_and_cross_plugin_reads_are_refused() {
    let registry = registry_with_assets("scheme-guard");
    for uri in [
        "astra-plugin://dev.astra.keys/../manifest.toml",
        "astra-plugin://dev.astra.keys/..\\manifest.toml",
    ] {
        let answer = answer_asset(&registry, uri);
        assert_eq!(answer.status(), 403, "{uri}");
    }
    // A root-anchored path is absolute on Unix and missing on Windows: refused
    // either way, with the exact status the platform's Path semantics give.
    let anchored = answer_asset(&registry, "astra-plugin://dev.astra.keys//etc/passwd");
    assert!(
        anchored.status() == 403 || anchored.status() == 404,
        "the anchored path never resolves: {}",
        anchored.status()
    );
}

#[test]
fn a_missing_file_or_unknown_plugin_is_a_404() {
    let registry = registry_with_assets("scheme-missing");
    assert_eq!(
        answer_asset(&registry, "astra-plugin://dev.astra.keys/nope.html").status(),
        404
    );
    assert_eq!(
        answer_asset(&registry, "astra-plugin://dev.astra.ghost/keys.html").status(),
        404
    );
    assert_eq!(answer_asset(&registry, "astra-plugin://bare").status(), 400);
}
