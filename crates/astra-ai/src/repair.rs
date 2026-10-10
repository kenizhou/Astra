// SPDX-License-Identifier: GPL-3.0-only
//! The structured-output repair helper shared by every structured AI
//! caller: the desktop host's completion client and the app layer's
//! generation paths.

/// The first balanced `{...}` block, string-aware: braces and quotes inside
/// JSON string literals do not disturb the depth count and backslash
/// escapes are honoured (kylins' `first_balanced_object`). The recovery
/// path for a model that answered with prose around the JSON: the caller
/// retries its parse once with the extracted slice.
#[must_use]
pub fn repair_json(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&raw[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}
