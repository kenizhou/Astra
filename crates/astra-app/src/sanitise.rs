// SPDX-License-Identifier: GPL-3.0-only
//! HTML sanitisation for the reading pane.
//!
//! The engine returns a message's `text/html` part unsanitised: mail HTML is
//! hostile input. Before any host renders it, this module strips it to a safe,
//! inert subset with [`ammonia`], one hardened configuration shared by every
//! surface rather than re-implemented per platform, so a malicious message can
//! neither run script, escape its frame, nor navigate the host away.
//!
//! The configuration ports mail-calendar's sanitiser: what is removed (scripts
//! and their content, event handlers, `iframe`/`object`/`embed`/`form`, `base`/
//! `meta`, `link` so nothing is prefetched, and any URL scheme other than
//! http(s)/mailto/data/cid) and what is kept because stripping it makes real
//! mail illegible (presentational HTML and CSS: inline `style`, `style` blocks,
//! `class`, `id`, and the table layout attributes).
//!
//! Two transforms of Astra's own follow the ammonia pass, both pure functions
//! over the sanitised string: an inline `cid:` image is resolved into a
//! self-contained `data:` URI from the engine's decoded parts (skipped with alt
//! text over a size cap, so a huge image cannot balloon the snapshot), and a
//! remote `src` is rewritten to a `data-astra-src` placeholder and counted, so
//! nothing remote loads until the reader opts in. A remote reference inside CSS
//! (`url(...)` or `@import`) is neither rewritten nor counted: the reading
//! document's content security policy is what blocks that load, the same
//! boundary mail-calendar leans on.

use std::{collections::HashSet, sync::OnceLock};

use ammonia::Builder;
use engine_api::InlinePart;

/// The byte ceiling an inline image may occupy and still resolve into the body
/// as a `data:` URI. Inline images ride inside the snapshot string that crosses
/// the IPC, so the cap bounds what one message's body can cost; a part over it
/// is skipped with alt text rather than inlined.
pub(crate) const INLINE_IMAGE_CAP_BYTES: usize = 1024 * 1024;

/// The alt text an over-cap inline image is skipped with.
const SKIPPED_ALT: &str = "alt=\"inline image skipped\"";

/// The sanitised body, and whether it references inline `cid:` resources the
/// caller may need to resolve.
pub(crate) struct Clean {
    /// The inert, safe-to-render HTML (a body fragment).
    pub html: String,
    /// Whether the body references an inline `cid:` resource: the signal for the
    /// reading pipeline to fetch the message's inline parts, so an open with no
    /// inline images never pays for the parts fetch.
    pub has_cid_references: bool,
}

/// Sanitises a message's raw `text/html` into an inert subset that keeps real
/// mail legible, per the module's configuration.
pub(crate) fn sanitise(html: &str) -> Clean {
    let cleaned = sanitizer().clean(html).to_string();
    Clean {
        has_cid_references: references_cid(&cleaned),
        html: cleaned,
    }
}

/// The shared, immutable [`Builder`]: its allowlists never change, so it is
/// configured once and reused for every message open (ammonia's `clean` takes
/// `&self`), rather than rebuilt per call.
fn sanitizer() -> &'static Builder<'static> {
    static SANITIZER: OnceLock<Builder<'static>> = OnceLock::new();
    SANITIZER.get_or_init(|| {
        // Only safe URL schemes survive, notably no `javascript:` or `vbscript:`.
        // Links use the launchable allowlist (http(s)/mailto); `data:` and `cid:`
        // are kept for inline `src` resources (and `data:` links are blocked
        // below: a link can carry an executable document). Remote http(s) image
        // sources are kept by the sanitiser and gated afterwards by the remote
        // rewrite, which is this product's boundary where mail-calendar's host
        // CSP is.
        let schemes: HashSet<&str> = ["http", "https", "mailto", "data", "cid"].into();
        let mut builder = Builder::new();
        builder
            .url_schemes(schemes)
            // Keep presentational CSS (inline and blocks) and the attributes mail
            // uses for layout; the reading document, not stripping, contains it.
            .add_tags(["style"])
            .rm_clean_content_tags(["style"])
            // Drop <title>'s text too, not just the tag; otherwise the document
            // title (often a copy of the subject) leaks in as a stray line of
            // body text.
            .add_clean_content_tags(["title"])
            // Presentational attributes mail uses for table layout. `border`,
            // `cellspacing`, and `cellpadding` matter for correctness, not just
            // looks: responsive mail commonly sets `border="0"` and then a mobile
            // @media rule paints a spurious box without it. `id` is a selector
            // hook exactly like `class`, kept unprefixed: a newsletter's mobile
            // layout routinely hangs off it alone, and nothing here collides
            // (the reading document holds one message and no script).
            .add_generic_attributes([
                "style",
                "class",
                "id",
                "align",
                "valign",
                "bgcolor",
                "border",
                "cellspacing",
                "cellpadding",
                "width",
                "height",
            ])
            .attribute_filter(|element, attribute, value| match (element, attribute) {
                // A `data:` link can carry an executable document (for example
                // data:text/html); links may only point at real http(s)/mailto
                // targets.
                ("a", "href") if is_data_uri(value) => None,
                _ => Some(value.into()),
            });
        builder
    })
}

/// Whether `value` is an inline `data:` URI (scheme-only check, leading space
/// tolerant).
///
/// Uses `get(..5)` rather than slicing `[..5]`: the value is hostile input, and a
/// byte slice would panic when byte 5 falls inside a multi-byte char (a link
/// `href` with a non-ASCII character), unwinding through the whole sanitise pass.
fn is_data_uri(value: &str) -> bool {
    value
        .trim_start()
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
}

/// The `src="cid:` token (lowercased) that fronts every inline-image reference
/// in ammonia's serialised output (attributes are normalised to double quotes).
/// `cid:` survives the sanitiser only on an image `src`, so anchoring on `src="`
/// confines the rewrite to image sources and never touches a surviving `cid:`
/// elsewhere.
const CID_SRC_MARKER: &str = "src=\"cid:";

/// Whether the sanitised HTML references an inline `cid:` image source. Matched
/// case-insensitively because a sender may author `CID:` or `SRC=`, which
/// ammonia normalises on output anyway.
fn references_cid(html: &str) -> bool {
    html.to_ascii_lowercase().contains(CID_SRC_MARKER)
}

/// Rewrites each resolvable inline `cid:` image reference in `html` to a
/// self-contained `data:` URI built from `parts`, so the reading document
/// renders it with no network load and no opt-in: an inline image is part of
/// the message, not remote content.
///
/// Only an `<img src="cid:ID">` is rewritten (the sole place the sanitiser keeps
/// `cid:`), and only when `ID` resolves to an inline part whose media type is a
/// safe `image/*` and whose bytes fit the size cap (defensive: the rewrite must
/// never emit a non-image `data:` document, nor a multi-megabyte one). An
/// unresolved id, a non-image part, or an over-cap part is skipped: the first
/// leaves the original `cid:` reference untouched as an inert broken image, the
/// latter two replace the reference with alt text naming the skip (where the
/// sender's own `alt` attribute precedes it, a reader shows that instead, which
/// is the finer text). Operates on byte indices found at ASCII boundaries, so
/// it never slices a multi-byte char in hostile input.
pub(crate) fn inline_cid_images(html: &str, parts: &[InlinePart]) -> String {
    if parts.is_empty() {
        return html.to_owned();
    }
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find(CID_SRC_MARKER) {
        let token_start = cursor + rel; // index of `s` in `src="cid:`
        let value_start = token_start + CID_SRC_MARKER.len(); // first char of the id
                                                              // The id runs to the next double quote that closes the attribute; ammonia
                                                              // escapes any literal `"` in a value to `&quot;`, so the first quote is
                                                              // the real terminator.
        let Some(quote_rel) = html[value_start..].find('"') else {
            break; // malformed (no closing quote): copy the remainder untouched.
        };
        let value_end = value_start + quote_rel;
        let token_end = value_end + 1; // past the closing quote
        let cid = html[value_start..value_end].trim();

        out.push_str(&html[cursor..token_start]);
        match resolve_image(parts, cid) {
            Resolved::Inline(media_type, bytes) => {
                out.push_str("src=\"data:");
                out.push_str(&media_type);
                out.push_str(";base64,");
                base64_encode_into(bytes, &mut out);
                out.push('"');
            }
            Resolved::TooLarge => out.push_str(SKIPPED_ALT),
            // Unresolved or non-image: leave the original `src="cid:ID"` as-is.
            Resolved::Missing => out.push_str(&html[token_start..token_end]),
        }
        cursor = token_end;
    }
    out.push_str(&html[cursor..]);
    out
}

/// The outcome of resolving one `cid:` token against the inline parts.
enum Resolved<'a> {
    /// A resolvable, safe, in-cap image: its media type and bytes.
    Inline(String, &'a [u8]),
    /// A resolvable image over the size cap: skipped with alt text.
    TooLarge,
    /// No part matches, or the part is not a safe image: leave the reference.
    Missing,
}

/// Resolves a `cid:` token against `parts`. The id match is exact and
/// case-sensitive (RFC 2392: a `cid:` references a case-sensitive
/// `Content-ID`).
fn resolve_image<'a>(parts: &'a [InlinePart], cid: &str) -> Resolved<'a> {
    let Some(part) = parts.iter().find(|part| part.content_id() == cid) else {
        return Resolved::Missing;
    };
    let Some(media_type) = image_media_type(part.media_type()) else {
        return Resolved::Missing;
    };
    let bytes = part.bytes();
    if bytes.len() > INLINE_IMAGE_CAP_BYTES {
        return Resolved::TooLarge;
    }
    Resolved::Inline(media_type, bytes)
}

/// The normalised `image/<subtype>` media type to place in a `data:` URI, or
/// `None` if `media_type` is not an image type or carries any character unsafe
/// in a `data:` URL.
///
/// Restricting to `image/*` keeps the rewrite from ever producing an executable
/// or document `data:` URI, and the subtype allowlist (RFC 6838
/// restricted-name characters) excludes `;`, `,`, and `"`: the characters that
/// could inject a `data:` parameter or break out of the `src` attribute.
/// `image/svg+xml` is allowed: an SVG loaded via `img` renders non-scripted,
/// and the reading document carries no script source regardless.
fn image_media_type(media_type: &str) -> Option<String> {
    let lower = media_type.trim().to_ascii_lowercase();
    let subtype = lower.strip_prefix("image/")?;
    let safe = !subtype.is_empty()
        && subtype
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-' | b'_'));
    safe.then_some(lower)
}

/// Appends the standard (RFC 4648) base64 encoding of `bytes` to `out`.
/// Hand-rolled to keep the core dependency-light; encoding is a fixed
/// 3-byte-to-4-char mapping with `=` padding.
fn base64_encode_into(bytes: &[u8], out: &mut String) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.reserve(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b1 = chunk.get(1).copied();
        let b2 = chunk.get(2).copied();
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(b1.unwrap_or(0)) << 8)
            | u32::from(b2.unwrap_or(0));
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if b1.is_some() {
            ALPHABET[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if b2.is_some() {
            ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
}

/// The gated body: the HTML with every remote source now a placeholder, and how
/// many remote references are blocked until the reader opts in.
pub(crate) struct Gated {
    /// The HTML whose remote sources are `data-astra-src` placeholders.
    pub html: String,
    /// How many remote sources were rewritten, one per placeholder: the count a
    /// surface offers the "show remote images" toggle for.
    pub remote_image_count: u32,
}

/// Rewrites every remote source reference in `html` to a `data-astra-src`
/// placeholder carrying the original URL, and counts them, so nothing remote
/// loads until a surface resolves the placeholders on the reader's opt-in.
///
/// A remote source is an `src` attribute value over `http(s)` or a
/// protocol-relative `//`; the value is carried verbatim (the sanitiser has
/// already escaped it). Local sources are untouched: `data:` is already-inlined
/// bytes and `cid:` an inert unresolved reference. Only `src` is rewritten: an
/// `href` fetches nothing until clicked, and CSS-borne `url(...)` loads are the
/// reading document's content security policy to block.
pub(crate) fn gate_remote_sources(html: &str) -> Gated {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut count: u32 = 0;
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find("src=\"") {
        let token_start = cursor + rel; // index of `s` in `src="`
        let value_start = token_start + "src=\"".len();
        let Some(quote_rel) = html[value_start..].find('"') else {
            break; // malformed (no closing quote): copy the remainder untouched.
        };
        let value_end = value_start + quote_rel;
        let token_end = value_end + 1;
        let value = &html[value_start..value_end];
        let remote = {
            let trimmed = value.trim_start();
            let scheme_remote = trimmed
                .get(..4)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http"));
            scheme_remote || trimmed.starts_with("//")
        };
        out.push_str(&html[cursor..token_start]);
        if remote {
            out.push_str("data-astra-src=\"");
            out.push_str(value);
            out.push('"');
            count += 1;
        } else {
            out.push_str(&html[token_start..token_end]);
        }
        cursor = token_end;
    }
    out.push_str(&html[cursor..]);
    Gated {
        html: out,
        remote_image_count: count,
    }
}

#[cfg(test)]
#[path = "sanitise_tests.rs"]
mod tests;
