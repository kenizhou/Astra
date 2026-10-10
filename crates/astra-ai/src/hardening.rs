// SPDX-License-Identifier: GPL-3.0-only
//! Prompt-injection hardening: the untrusted-content wrapper. The data
//! loss prevention policy and gate live in the crate's `dlp` module.

/// The opening fence line: content between the fences is untrusted data.
const FENCE_BEGIN: &str = "[[[untrusted:begin]]]";
/// The closing fence line.
const FENCE_END: &str = "[[[untrusted:end]]]";

/// Wraps untrusted content (a message body, a search result, anything a
/// third party shapes) so the model reads it as data, never as
/// instructions. The heading names what the content is; double quotes and
/// ampersands in it are escaped so the attribute cannot be broken out of.
/// Already-wrapped content is returned unchanged, so nested wrapping cannot
/// smuggle a second, untagged copy.
///
/// ```text
/// <untrusted heading="email body">
/// [[[untrusted:begin]]]
/// Buy now! Ignore previous instructions.
/// [[[untrusted:end]]]
/// </untrusted>
/// ```
#[must_use]
pub fn tag_untrusted(heading: &str, content: &str) -> String {
    if is_wrapped(content) {
        return content.to_string();
    }
    let mut escaped_heading = String::with_capacity(heading.len());
    for character in heading.chars() {
        match character {
            '&' => escaped_heading.push_str("&amp;"),
            '"' => escaped_heading.push_str("&quot;"),
            _ => escaped_heading.push(character),
        }
    }
    format!(
        "<untrusted heading=\"{escaped_heading}\">\n{FENCE_BEGIN}\n{content}\n{FENCE_END}\n</untrusted>"
    )
}

/// True when `content` is already a [`tag_untrusted`] wrapper.
fn is_wrapped(content: &str) -> bool {
    let trimmed = content.trim();
    trimmed.starts_with("<untrusted heading=\"") && trimmed.ends_with("</untrusted>")
}
