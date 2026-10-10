// SPDX-License-Identifier: GPL-3.0-only
//! Data loss prevention: the span redactor and the policy gate every
//! mail-carrying prompt passes through. The redactor masks what a message
//! can leak (email addresses, phone-shaped digit runs, long digit runs)
//! before the content wraps for the model; the policy decides, per message,
//! whether the content leaves the machine at all.

/// The mask character: one per redacted character, so a masked span keeps
/// its length and the layout around it survives.
const MASK: char = '*';

/// The fewest digits a bare run needs to be sensitive on its own: the
/// shape of card and account numbers, longer than any number prose quotes
/// for meaning.
const LONG_RUN_DIGITS: usize = 12;

/// The digit band a separated run may carry and count as phone-shaped:
/// from local minima to the E.164 ceiling.
const PHONE_MIN_DIGITS: usize = 7;
const PHONE_MAX_DIGITS: usize = 15;

/// The fewest characters a top level domain must carry for an address
/// span to count: `a@b.c` is prose, not an address.
const TLD_MIN_CHARS: usize = 2;

/// Masks the sensitive spans in `content`: email addresses, phone-shaped
/// digit runs (digits joined by separator gaps), and long digit runs
/// (twelve digits or more, separators or none). Every character of a
/// sensitive span becomes the mask, lengths preserved: the pin. A digit run
/// an ASCII letter immediately flanks rides inside a word and stays: the
/// adjacency pin. Masked spans carry no digits and no addresses, so
/// redacting already-masked content changes nothing.
#[must_use]
pub fn redact(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut spans = email_spans(bytes);
    spans.extend(number_spans(bytes));
    let mut masked = String::with_capacity(content.len());
    let mut at = 0usize;
    for (start, end) in merge(spans) {
        if start > at {
            masked.push_str(&content[at..start]);
        }
        for _ in start..end {
            masked.push(MASK);
        }
        at = end;
    }
    masked.push_str(&content[at..]);
    masked
}

/// The gate: one policy's decision over one message's content. Allow passes
/// the content through untouched, redact masks it then allows it, and block
/// refuses it outright: the caller skips the message's AI stages.
#[must_use]
pub fn apply(policy: DlpPolicy, content: &str) -> DlpOutcome {
    match policy {
        DlpPolicy::Allow => DlpOutcome::Allowed {
            content: content.to_owned(),
        },
        DlpPolicy::Redact => DlpOutcome::Allowed {
            content: redact(content),
        },
        DlpPolicy::Block => DlpOutcome::Blocked,
    }
}

/// The data loss prevention posture for content leaving the machine. One
/// policy stands for the whole pass, read once at assembly, and is applied
/// per message before any stage prompt is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DlpPolicy {
    /// Content passes through untouched.
    Allow,
    /// Sensitive spans are masked before the content rides any prompt.
    Redact,
    /// The call is refused outright: the message's AI stages do not run.
    Block,
}

impl DlpPolicy {
    /// The environment variable the policy comes from.
    pub const ENV_VAR: &str = "ASTRA_DLP";

    /// Reads the policy from the process environment. An absent or blank
    /// value is the default allow; a present value must be one of the three
    /// wire words, and anything else is a loud error rather than a silent
    /// allow.
    ///
    /// # Errors
    /// [`DlpError::Malformed`] for any value the wire words do not name.
    pub fn from_env() -> Result<Self, DlpError> {
        Self::from_value(std::env::var(Self::ENV_VAR).ok().as_deref())
    }

    /// The parsing seam behind [`DlpPolicy::from_env`]: `raw` is the
    /// variable's value, absent when unset. The wire words are the policy
    /// names, matched case-insensitively after trimming.
    ///
    /// # Errors
    /// As [`DlpPolicy::from_env`].
    pub fn from_value(raw: Option<&str>) -> Result<Self, DlpError> {
        let Some(raw) = raw else {
            return Ok(Self::Allow);
        };
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "allow" => Ok(Self::Allow),
            "redact" => Ok(Self::Redact),
            "block" => Ok(Self::Block),
            _ => Err(DlpError::Malformed {
                value: raw.trim().to_owned(),
            }),
        }
    }
}

/// What the gate did with one message's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DlpOutcome {
    /// The content may ride, transformed when the policy redacts.
    Allowed {
        /// The content as the stages will see it.
        content: String,
    },
    /// The content may not ride: the caller skips the message's stages.
    Blocked,
}

/// Failures of policy parsing, loud by design: a misconfigured policy is
/// never silently treated as allow.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DlpError {
    /// The value the variable carried, rejected with the accepted words.
    #[error(
        "{}: unknown policy {value:?}; expected allow, redact, or block",
        DlpPolicy::ENV_VAR
    )]
    Malformed {
        /// The offending value, as the variable spelled it.
        value: String,
    },
}

/// The spans of every email address: an `@` with a local part before it
/// and a dotted domain with an alphabetic tail after it. All the character
/// classes are ASCII, so the scan walks bytes and the spans land on char
/// boundaries.
fn email_spans(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for (i, &byte) in bytes.iter().enumerate() {
        if byte != b'@' {
            continue;
        }
        let mut start = i;
        while start > 0 && is_local_byte(bytes[start - 1]) {
            start -= 1;
        }
        if start == i {
            continue;
        }
        let mut end = i + 1;
        while end < bytes.len() && is_domain_byte(bytes[end]) {
            end += 1;
        }
        let domain = &bytes[i + 1..end];
        let Some(dot) = domain.iter().rposition(|&b| b == b'.') else {
            continue;
        };
        let tld = &domain[dot + 1..];
        if dot == 0 || tld.len() < TLD_MIN_CHARS || !tld.iter().all(u8::is_ascii_alphabetic) {
            continue;
        }
        spans.push((start, end));
    }
    spans
}

/// The bytes an address's local part may carry.
fn is_local_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'%' | b'+' | b'-')
}

/// The bytes an address's domain may carry.
fn is_domain_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')
}

/// The spans of every number token: digit groups joined by separator gaps,
/// an optional leading plus. A token is sensitive when it carries twelve
/// digits or more (the long run, separators or none) or when it carries
/// separator gaps and its digit count sits in the phone band. A token an
/// ASCII letter immediately flanks is part of a word and stays.
fn number_spans(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = if i > 0 && bytes[i - 1] == b'+' {
            i - 1
        } else {
            i
        };
        let mut end = i;
        let mut digits = 0usize;
        let mut separators = 0usize;
        loop {
            let group = end;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            digits += end - group;
            // One join: at most two separator bytes followed by another
            // digit group, so an area code's closing bracket and the space
            // after it read as one gap.
            let mut join = end;
            while join < bytes.len() && join - end < 2 && is_group_separator(bytes[join]) {
                join += 1;
            }
            if join < bytes.len() && bytes[join].is_ascii_digit() {
                separators += join - end;
                end = join;
            } else {
                break;
            }
        }
        let flanked_by_letters = (start > 0 && bytes[start - 1].is_ascii_alphabetic())
            || (end < bytes.len() && bytes[end].is_ascii_alphabetic());
        let phone_shaped =
            separators > 0 && (PHONE_MIN_DIGITS..=PHONE_MAX_DIGITS).contains(&digits);
        if !flanked_by_letters && (digits >= LONG_RUN_DIGITS || phone_shaped) {
            spans.push((start, end));
        }
        i = end;
    }
    spans
}

/// The separator bytes that may join digit groups in a phone-shaped token.
fn is_group_separator(byte: u8) -> bool {
    matches!(byte, b' ' | b'-' | b'.' | b'(' | b')')
}

/// Sorts the spans and folds every overlap into one, so an address whose
/// local part is itself a long digit run masks as the union of the two.
fn merge(mut spans: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some((_, last_end)) if start <= *last_end => {
                if end > *last_end {
                    *last_end = end;
                }
            }
            _ => merged.push((start, end)),
        }
    }
    merged
}
