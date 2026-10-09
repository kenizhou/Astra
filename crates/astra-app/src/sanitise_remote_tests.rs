// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the sanitiser's remote gate: which references are rewritten to
//! placeholders and counted (remote `src` attributes, remote CSS `url(...)` and
//! `@import`), and which are untouched (local `data:`/`cid:`, relative, plain
//! links). The CSS vectors mirror the ones mail-calendar's remote detector scans
//! for; the placeholder rewrite itself is Astra's own, per the SP2c design.

// The gate's functions come from the sanitiser module (this file's
// grandparent), the inline-part helper from the tests module it splits from.
use super::super::{gate_remote_sources, inline_cid_images, sanitise};
use super::part;

#[test]
fn remote_sources_are_rewritten_to_placeholders_and_counted() {
    let gated = gate_remote_sources(
        r#"<p>x</p><img src="https://tracker.example/p.gif?id=42"><img src="//cdn.example/a.png">"#,
    );
    assert!(
        gated
            .html
            .contains(r#"data-astra-src="https://tracker.example/p.gif?id=42""#),
        "{}",
        gated.html
    );
    assert!(
        gated
            .html
            .contains(r#"data-astra-src="//cdn.example/a.png""#),
        "{}",
        gated.html
    );
    // No plain src with a remote or protocol-relative value survives.
    assert!(!gated.html.contains(r#" src="http"#), "{}", gated.html);
    assert!(!gated.html.contains(r#" src="//"#), "{}", gated.html);
    assert_eq!(gated.remote_image_count, 2, "one per rewritten source");
    assert!(gated.html.contains("<p>x</p>"), "the rest is untouched");
}

#[test]
fn remote_sources_rewrite_case_insensitively() {
    // A sender may author SRC= or HTTP://; the scan matches either and the value
    // is carried verbatim.
    let gated = gate_remote_sources(r#"<img SRC="HTTP://x.example/a.png">"#);
    assert!(
        gated
            .html
            .contains(r#"data-astra-src="HTTP://x.example/a.png""#),
        "{}",
        gated.html
    );
    assert_eq!(gated.remote_image_count, 1);
}

#[test]
fn local_sources_are_untouched_by_the_remote_gate() {
    // data: (already inlined bytes) and cid: (an unresolved inert reference) load
    // nothing remote, so neither is rewritten nor counted.
    let html = r#"<img src="data:image/png;base64,aGVsbG8="><img src="cid:logo@x">"#;
    let gated = gate_remote_sources(html);
    assert_eq!(gated.html, html);
    assert_eq!(gated.remote_image_count, 0);
}

#[test]
fn a_plain_remote_link_is_not_a_load_and_is_not_counted() {
    // An <a href> fetches nothing until clicked; it stays a link, uncounted.
    let gated = gate_remote_sources(r#"<a href="https://x.example">l</a>"#);
    assert!(
        gated.html.contains(r#"href="https://x.example""#),
        "{}",
        gated.html
    );
    assert!(!gated.html.contains("data-astra-src"), "{}", gated.html);
    assert_eq!(gated.remote_image_count, 0);
}

#[test]
fn inlined_and_remote_images_carry_the_same_body_through_both_transforms() {
    // The pipeline's composition: sanitise, resolve the cid: images, then gate the
    // remote references, in that order, over one body that carries all three kinds.
    let raw = concat!(
        r#"<p>Body</p><img src="cid:logo@x">"#,
        r#"<img src="https://tracker.example/p.png">"#,
        r#"<img src="data:image/gif;base64,R0lGOD">"#,
    );
    let cleaned = sanitise(raw);
    let inlined = inline_cid_images(&cleaned.html, &[part("logo@x", "image/png", b"hello")]);
    let gated = gate_remote_sources(&inlined);
    assert!(
        gated
            .html
            .contains("src=\"data:image/png;base64,aGVsbG8=\""),
        "{}",
        gated.html
    );
    assert!(
        gated
            .html
            .contains(r#"data-astra-src="https://tracker.example/p.png""#),
        "{}",
        gated.html
    );
    assert!(
        gated.html.contains("src=\"data:image/gif;base64,R0lGOD\""),
        "{}",
        gated.html
    );
    assert_eq!(gated.remote_image_count, 1);
}

#[test]
fn a_remote_css_url_in_an_inline_style_is_rewritten_and_counted() {
    // A CSS background over https fetches just as an img does; it is gated the
    // same way, behind the placeholder scheme, and counted.
    let gated =
        gate_remote_sources(r#"<div style="background:url(https://x.example/a.png)">z</div>"#);
    assert!(
        gated
            .html
            .contains("url(data-astra-style-url:https://x.example/a.png)"),
        "{}",
        gated.html
    );
    assert!(!gated.html.contains("url(https://"), "{}", gated.html);
    assert_eq!(gated.remote_image_count, 1);
}

#[test]
fn a_remote_css_url_inside_a_style_block_is_rewritten_and_counted() {
    // The variants a flat scan misses: whitespace after the paren, a
    // single-quoted url(), and one buried in an @media rule inside <style>.
    let gated = gate_remote_sources(
        "<style>@media all{body{background:url( https://x.example/a.png )}}\
             .a{background:url('https://y.example/b.png')}</style><p>x</p>",
    );
    assert!(
        gated
            .html
            .contains("data-astra-style-url:https://x.example/a.png"),
        "{}",
        gated.html
    );
    assert!(
        gated
            .html
            .contains("data-astra-style-url:https://y.example/b.png"),
        "{}",
        gated.html
    );
    assert!(!gated.html.contains("url( https://"), "{}", gated.html);
    assert!(!gated.html.contains("url('https://"), "{}", gated.html);
    assert_eq!(gated.remote_image_count, 2);
}

#[test]
fn a_remote_import_is_rewritten_and_counted() {
    // @import in its string form and its url() form: both load remote CSS, both
    // are gated and counted, and the url() form is not counted twice.
    let gated = gate_remote_sources(concat!(
        r#"<style>@import "https://x.example/x.css";</style>"#,
        r#"<style>@import url(https://y.example/y.css);</style><p>x</p>"#,
    ));
    assert!(
        gated
            .html
            .contains(r"data-astra-style-url:https://x.example/x.css"),
        "{}",
        gated.html
    );
    assert!(
        gated
            .html
            .contains("url(data-astra-style-url:https://y.example/y.css)"),
        "{}",
        gated.html
    );
    assert_eq!(gated.remote_image_count, 2);
}

#[test]
fn remote_css_references_are_counted_alongside_remote_sources() {
    // The one count covers every kind of remote reference: a source and two CSS
    // loads are three blocked references.
    let gated = gate_remote_sources(concat!(
        r#"<img src="https://tracker.example/p.png">"#,
        r#"<div style="background:url(https://x.example/a.png)">z</div>"#,
        r#"<style>@import "https://y.example/y.css";</style>"#,
    ));
    assert_eq!(gated.remote_image_count, 3);
    assert!(
        gated
            .html
            .contains(r#"data-astra-src="https://tracker.example/p.png""#),
        "{}",
        gated.html
    );
}

#[test]
fn local_css_urls_are_untouched_and_uncounted() {
    // data: and cid: inside url() load nothing remote, and a relative url() has
    // no remote scheme to load: none is rewritten or counted.
    let html = concat!(
        r#"<style>.a{background:url(data:image/png;base64,aGVsbG8=)}</style>"#,
        r#"<div style="list-style:url(cid:dot@x)">z</div>"#,
        r#"<style>.b{background:url(../local/a.png)}</style>"#,
    );
    let gated = gate_remote_sources(html);
    assert_eq!(gated.html, html);
    assert_eq!(gated.remote_image_count, 0);
}

#[test]
fn a_protocol_relative_css_url_is_rewritten_and_counted() {
    let gated = gate_remote_sources(r#"<div style="background:url(//cdn.example/a.png)">z</div>"#);
    assert!(
        gated
            .html
            .contains("url(data-astra-style-url://cdn.example/a.png)"),
        "{}",
        gated.html
    );
    assert_eq!(gated.remote_image_count, 1);
}

#[test]
fn a_css_reference_that_never_closes_is_copied_untouched() {
    // Hostile input: an unterminated url( must not panic or eat the document.
    // The broken declaration is invalid CSS that no engine loads, so it is left
    // exactly as it stands rather than rewritten past an extent it does not have.
    let html = r"<style>.a{background:url(https://x.example/a.png";
    let gated = gate_remote_sources(html);
    assert_eq!(gated.html, html);
    assert_eq!(gated.remote_image_count, 0);
}
