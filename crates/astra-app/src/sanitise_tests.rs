// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the reading body's sanitiser: what it strips, what it keeps because
//! stripping it makes real mail illegible, how an inline `cid:` image resolves into a
//! `data:` URI, and how a remote source becomes a placeholder. The strip-and-keep
//! vectors port mail-calendar's sanitisation tests; the inline size cap and the remote
//! placeholder rewrite are Astra's own, per the SP2c design.

use engine_api::InlinePart;

use super::{gate_remote_sources, inline_cid_images, sanitise, INLINE_IMAGE_CAP_BYTES};

/// An inline part, the shape the engine's parts read hands the rewrite.
fn part(cid: &str, media: &str, bytes: &[u8]) -> InlinePart {
    InlinePart::new(cid, media, bytes.to_vec())
}

#[test]
fn strips_script_tags_and_their_content() {
    let out = sanitise("<p>hi</p><script>steal(document.cookie)</script>").html;
    assert!(out.contains("hi"));
    assert!(!out.contains("<script"));
    assert!(!out.contains("steal"));
}

#[test]
fn strips_inline_event_handlers() {
    let out = sanitise(r#"<a href="https://ok.example" onclick="evil()">x</a>"#).html;
    assert!(!out.contains("onclick"));
    assert!(!out.contains("evil"));
    assert!(out.contains("https://ok.example"));
}

/// A `<link rel=dns-prefetch>` host is resolved by some web engines as the document
/// parses, so stripping the element is what keeps a message from announcing that it
/// was opened to the sender's resolver.
#[test]
fn strips_link_elements_so_no_host_is_prefetched() {
    let out = sanitise(r#"<link rel="dns-prefetch" href="//tracker.example"><p>hi</p>"#).html;
    assert!(out.contains("hi"));
    assert!(!out.contains("<link"));
    assert!(!out.contains("tracker.example"));
}

#[test]
fn preserves_inline_css_so_mail_is_legible() {
    // Inline colours and fonts must survive: the reading document, not stripping,
    // is the security boundary.
    let out = sanitise(r#"<p style="color:#0000ff;font-weight:bold">blue</p>"#).html;
    assert!(out.contains("blue"));
    assert!(out.contains("style="));
    assert!(out.contains("#0000ff") || out.contains("color"));
}

#[test]
fn preserves_style_blocks_and_classes() {
    // Marketing mail puts media queries and class rules in <style>; child
    // combinators and @media must survive unescaped, or the CSS breaks.
    let out = sanitise(
        "<style>@media (max-width:600px){.x{color:blue}} .a>.b{margin:0}</style>\
             <p class=\"x\">y</p>",
    )
    .html;
    assert!(out.contains("<style"));
    assert!(out.contains("@media"));
    assert!(out.contains(".a>.b") || out.contains(".a >.b") || out.contains(".a > .b"));
    assert!(out.contains("class=\"x\""));
}

#[test]
fn preserves_presentational_table_attributes() {
    // Responsive mail relies on `border="0"` keeping border-width 0 so a mobile
    // `border-style: solid !important` rule stays invisible; dropping it paints a box.
    let out = sanitise(
        r#"<table border="0" cellspacing="0" cellpadding="0" width="600"><tr><td>x</td></tr></table>"#,
    )
    .html;
    assert!(out.contains(r#"border="0""#));
    assert!(out.contains(r#"cellspacing="0""#));
    assert!(out.contains(r#"cellpadding="0""#));
}

#[test]
fn preserves_the_id_a_responsive_media_query_selects_on() {
    // A newsletter's whole mobile layout hangs off `id`: the container table is a
    // fixed 600px and only the @media rule below narrows it. Strip the attribute
    // and the rule matches nothing, so the message lays out off the right edge.
    let out = sanitise(
        "<style>@media only screen and (max-width:480px){\
             table[id=templateContainer]{width:100% !important}}</style>\
         <table id=\"templateContainer\" width=\"600\"><tr><td id=\"bodyCell\">x</td></tr></table>",
    )
    .html;
    assert!(out.contains(r#"id="templateContainer""#));
    assert!(out.contains(r#"id="bodyCell""#));
}

#[test]
fn drops_the_document_title_text() {
    // The title (often a subject copy) must not leak in as body text, but a <style>
    // sibling in <head> must still survive.
    let out = sanitise(
        "<head><title>Subject leak</title><style>.x{color:red}</style></head>\
             <body><p>visible</p></body>",
    )
    .html;
    assert!(!out.contains("Subject leak"));
    assert!(out.contains("visible"));
    assert!(out.contains(".x{color:red}"));
}

#[test]
fn strips_iframes_and_objects() {
    let out = sanitise(r#"<iframe src="https://x.example"></iframe><b>kept</b>"#).html;
    assert!(out.contains("kept"));
    assert!(!out.contains("iframe"));
}

#[test]
fn blocks_data_uri_links() {
    // A data: link can carry an executable document; links may only point at real
    // http(s)/mailto targets.
    let out = sanitise(r#"<a href="data:text/html,<script>alert(1)</script>">x</a>"#).html;
    assert!(!out.contains("data:text/html"));
}

#[test]
fn strips_custom_scheme_links() {
    // A custom app-scheme href is not in the launchable allowlist, so the
    // sanitiser drops it and the link cannot even be clicked.
    let out = sanitise(r#"<a href="myapp://home">open</a>"#).html;
    assert!(out.contains("open"), "the link text survives");
    assert!(!out.contains("myapp://"), "the unlaunchable href is gone");
}

#[test]
fn does_not_panic_on_non_ascii_link_href() {
    // The data: link check must not byte-slice across a UTF-8 char boundary: a
    // link href with a multi-byte char around byte 5 must not panic the pass.
    assert!(sanitise(r#"<a href="abcdé fghij">x</a>"#)
        .html
        .contains('x'));
    let _ = sanitise("<a href=\"aaa😀bbb\">y</a>");
    let _ = sanitise("<a href=\"é\">z</a>");
}

#[test]
fn empty_or_textonly_input_is_safe() {
    assert_eq!(sanitise("").html, "");
    assert_eq!(sanitise("just text").html, "just text");
    assert!(!sanitise("just text").has_cid_references);
}

#[test]
fn sanitise_flags_inline_cid_references() {
    // A cid: image source sets the flag so the pipeline fetches inline parts; a
    // body with none (and a plain remote image) does not.
    assert!(sanitise(r#"<img src="cid:logo@x">"#).has_cid_references);
    assert!(!sanitise(r#"<img src="https://x/a.png">"#).has_cid_references);
    assert!(!sanitise("<p>just text</p>").has_cid_references);
}

#[test]
fn inline_cid_resolves_to_a_data_uri_with_base64_bytes() {
    // `hello` base64-encodes to `aGVsbG8=`; the resolved <img> loads inline.
    let out = inline_cid_images(
        r#"<img src="cid:logo@x">"#,
        &[part("logo@x", "image/png", b"hello")],
    );
    assert!(
        out.contains("src=\"data:image/png;base64,aGVsbG8=\""),
        "{out}"
    );
    assert!(!out.contains("cid:"), "the cid reference is gone: {out}");
}

#[test]
fn inline_cid_leaves_unresolved_and_nonimage_refs_untouched() {
    let parts = [
        part("present@x", "image/gif", b"hello"),
        // A non-image inline part must never become a data: document.
        part("doc@x", "text/html", b"<script>evil()</script>"),
    ];
    // An id with no matching part stays a (broken) cid reference, as before.
    let missing = inline_cid_images(r#"<img src="cid:absent@x">"#, &parts);
    assert!(missing.contains(r#"src="cid:absent@x""#), "{missing}");
    // A matching but non-image part is left as cid: no data:text/html is emitted.
    let nonimage = inline_cid_images(r#"<img src="cid:doc@x">"#, &parts);
    assert!(nonimage.contains(r#"src="cid:doc@x""#), "{nonimage}");
    assert!(!nonimage.contains("data:"), "{nonimage}");
}

#[test]
fn inline_cid_resolves_every_reference() {
    let html = r#"<img src="cid:a"><p>x</p><img src="cid:b">"#;
    let out = inline_cid_images(
        html,
        &[
            part("a", "image/gif", b"hello"),
            part("b", "image/jpeg", b"world"),
        ],
    );
    assert!(out.contains("data:image/gif;base64,aGVsbG8="), "{out}");
    assert!(out.contains("data:image/jpeg;base64,d29ybGQ="), "{out}");
    assert!(!out.contains("cid:"), "{out}");
    assert!(
        out.contains("<p>x</p>"),
        "surrounding markup preserved: {out}"
    );
}

#[test]
fn inline_cid_allows_svg_but_rejects_injection_in_media_type() {
    // SVG via <img> is rendered non-scripted and the reading document carries no
    // script source, so it is allowed.
    let svg = inline_cid_images(
        r#"<img src="cid:s">"#,
        &[part("s", "image/svg+xml", b"<svg/>")],
    );
    assert!(svg.contains("data:image/svg+xml;base64,"), "{svg}");
    // A media type smuggling extra data: parameters or an attribute break is
    // rejected (not a safe subtype), leaving the reference inert.
    let evil = inline_cid_images(
        r#"<img src="cid:s">"#,
        &[part("s", "image/png\";onerror=alert(1)//", b"x")],
    );
    assert!(evil.contains(r#"src="cid:s""#), "{evil}");
    assert!(!evil.contains("onerror"), "{evil}");
}

#[test]
fn inline_cid_is_a_noop_without_parts_or_references() {
    assert_eq!(
        inline_cid_images("<p>no images</p>", &[]),
        "<p>no images</p>"
    );
    let parts = [part("x", "image/png", b"hello")];
    // Parts present but the body references none of them: unchanged.
    assert_eq!(
        inline_cid_images("<p>no images</p>", &parts),
        "<p>no images</p>"
    );
}

#[test]
fn an_oversized_inline_part_is_skipped_with_alt_text() {
    // Over the cap the bytes never enter the snapshot: the image is skipped and
    // the alt text says so, rather than a multi-megabyte data URI crossing the IPC.
    let big = vec![b'x'; INLINE_IMAGE_CAP_BYTES + 1];
    let out = inline_cid_images(
        r#"<img src="cid:big@x">"#,
        &[part("big@x", "image/png", &big)],
    );
    assert!(!out.contains("data:"), "{out}");
    assert!(!out.contains(r#"src="cid:big@x""#), "{out}");
    assert!(out.contains(r#"alt="inline image skipped""#), "{out}");
}

#[test]
fn an_inline_part_at_exactly_the_cap_still_resolves() {
    // The cap is inclusive: a part at the limit is the largest that inlines.
    let edge = vec![b'x'; INLINE_IMAGE_CAP_BYTES];
    let out = inline_cid_images(
        r#"<img src="cid:edge@x">"#,
        &[part("edge@x", "image/png", &edge)],
    );
    assert!(out.contains("data:image/png;base64,"), "{out}");
}

#[test]
fn inline_cid_never_panics_on_hostile_input() {
    let parts = [part("é@x", "image/png", b"hello")];
    // Multi-byte chars around the cid value, and a src="cid: with no closing
    // quote, must not slice a char boundary or panic.
    let _ = inline_cid_images(r#"<img alt="café" src="cid:é@x">"#, &parts);
    let _ = inline_cid_images(r#"<img src="cid:unterminated"#, &parts);
    let _ = inline_cid_images("src=\"cid:😀\"", &parts);
}

#[test]
fn base64_matches_rfc4648_vectors() {
    // The encoder the data: URIs are built with is the standard one.
    let out = inline_cid_images(r#"<img src="cid:v">"#, &[part("v", "image/png", b"foobar")]);
    assert!(out.contains("data:image/png;base64,Zm9vYmFy"), "{out}");
    let padded = inline_cid_images(r#"<img src="cid:p">"#, &[part("p", "image/png", b"foob")]);
    assert!(
        out.contains("Zm9vYmFy") || padded.contains("Zm9vYg=="),
        "{padded}"
    );
    assert!(
        padded.contains("data:image/png;base64,Zm9vYg=="),
        "{padded}"
    );
}

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
    // remote sources, in that order, over one body that carries all three kinds.
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
