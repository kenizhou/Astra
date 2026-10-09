// SPDX-License-Identifier: GPL-3.0-only
//! Reading sanitised message HTML as plain text: the fallback a quote takes when
//! the original carries an HTML body the engine could derive no text rendering
//! from. Split out of the compose arm so each file stays under the 500-line
//! limit, following the drafts module's own split.

/// Extracts readable plain text from sanitised message HTML: block-level
/// boundaries become line breaks, consecutive blank lines collapse, and script
/// and style content is dropped rather than emitted as prose. Ported from
/// mail-calendar's query text projection, and written defensively besides: it
/// faces only the sanitiser's inert subset, but an unterminated tag or a stray
/// `<` is treated as text-or-drop, never as a parse failure.
pub(super) fn to_plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        push_text(&mut out, &rest[..open]);
        let after = &rest[open + 1..];
        // A comment runs to `-->`; anything unterminated swallows the remainder,
        // which is the safe direction (drop, never emit markup as prose).
        if let Some(body) = after.strip_prefix("!--") {
            rest = body.split_once("-->").map_or("", |(_, tail)| tail);
            continue;
        }
        let Some((tag, tail)) = after.split_once('>') else {
            // An unterminated `<` is literal text, not a tag.
            push_text(&mut out, &rest[open..]);
            return out.trim().to_owned();
        };
        let name = tag_name(tag);
        if matches!(name.as_str(), "script" | "style") {
            let close = format!("</{name}");
            rest = tail
                .split_once(close.as_str())
                .and_then(|(_, after_close)| after_close.split_once('>'))
                .map_or("", |(_, tail)| tail);
            continue;
        }
        if is_break(&name) {
            push_break(&mut out);
        }
        rest = tail;
    }
    push_text(&mut out, rest);
    out.trim().to_owned()
}

/// The lowercased element name of a tag's inner text (`"/p class=x"` → `"p"`).
fn tag_name(tag: &str) -> String {
    tag.trim_start_matches('/')
        .trim_start()
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Whether an element boundary reads as a line break. A small, explicit set:
/// treating every unknown tag as a break would shred inline styling into
/// separate lines.
fn is_break(name: &str) -> bool {
    matches!(
        name,
        "br" | "p"
            | "div"
            | "li"
            | "ul"
            | "ol"
            | "tr"
            | "table"
            | "blockquote"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "pre"
    )
}

/// Appends a single newline, collapsing runs so a `</p><div>` pair is one
/// break, not two.
fn push_break(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

/// Appends a run of text with its entities decoded and its whitespace
/// collapsed.
fn push_text(out: &mut String, text: &str) {
    for ch in decode_entities(text).chars() {
        if ch == '\n' {
            push_break(out);
        } else if ch.is_whitespace() {
            if !out.ends_with(' ') && !out.ends_with('\n') && !out.is_empty() {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
}

/// Decodes the HTML entities that survive sanitisation: the named ones the
/// sanitiser emits, a non-breaking space, and numeric character references. An
/// unrecognized entity is left as written; showing `&foo;` is better than
/// silently deleting text a reader needs.
fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        // An entity is short; a `&` with no `;` within a few characters is a
        // literal ampersand.
        if let Some(end) = after.find(';').filter(|end| *end <= 8) {
            if let Some(ch) = decode_one(&after[..end]) {
                out.push(ch);
            } else {
                out.push('&');
                out.push_str(&after[..end]);
                out.push(';');
            }
            rest = &after[end + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// One entity body (between `&` and `;`) as a character, or `None` if
/// unrecognized.
fn decode_one(body: &str) -> Option<char> {
    match body {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let digits = body.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}
