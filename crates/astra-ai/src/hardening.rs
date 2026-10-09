// SPDX-License-Identifier: GPL-3.0-only
//! Prompt-injection hardening: the untrusted-content wrapper and the data
//! loss prevention policy vocabulary.

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

/// The data loss prevention posture for content leaving the machine.
///
/// SPA0 resolves every policy to [`DlpPolicy::Allow`]: redaction and
/// blocking enforcement land with the A1 hardening slice. The deferral is
/// this type's contract, not a silent gap; callers code against the
/// resolve step now so A1 changes enforcement, never call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DlpPolicy {
    /// Content passes through untouched.
    Allow,
    /// Sensitive spans are masked before the call (enforced from A1).
    Redact,
    /// The call is refused outright (enforced from A1).
    Block,
}

impl DlpPolicy {
    /// The posture this build enforces for `policy`. SPA0 always answers
    /// [`DlpPolicy::Allow`]; A1 makes [`DlpPolicy::Redact`] and
    /// [`DlpPolicy::Block`] bite.
    pub fn resolve(_policy: DlpPolicy) -> DlpPolicy {
        DlpPolicy::Allow
    }
}
