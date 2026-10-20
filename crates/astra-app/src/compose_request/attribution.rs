// SPDX-License-Identifier: GPL-3.0-only
//! The quote attribution's own formatting: the month spellings, the sender
//! and address display forms, and the instant rendering the attribution line
//! and its header block draw from, split from the compose request builder so
//! each file stays under the 500-line limit.

use engine_api::{EmailAddress, Message, UtcDateTime};

use super::ComposeKind;
use astra_composer::{QuoteAttribution, QuoteHeader};

/// The month names the attribution line's date spells out. The core has no
/// locale (the same rule that keeps the `Re:` prefix untranslated), so one
/// English spelling everywhere.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The attribution above a quoted original, in both renderings a host can flip
/// between: the one-line a reply shows ("On `<date>`, `<sender>` wrote:", or
/// just the sender when the original carries no instant) and the labelled
/// header block a forward's style shows, which is the same for either kind.
pub(super) fn attribution(kind: ComposeKind, original: &Message) -> QuoteAttribution {
    let sender = original
        .envelope
        .from
        .first()
        .map_or_else(String::new, display_address);
    let date = display_instant(original.received_at.or(original.sent_at));
    let line = match (kind, &date) {
        (ComposeKind::Forward, _) => "Forwarded message".to_owned(),
        (_, Some(date)) => format!("On {date}, {sender} wrote:"),
        (_, None) => format!("{sender} wrote:"),
    };
    let mut headers = vec![QuoteHeader {
        label: "From".to_owned(),
        value: sender,
    }];
    if let Some(date) = date {
        headers.push(QuoteHeader {
            label: "Sent".to_owned(),
            value: date,
        });
    }
    let to = display_addresses(&original.envelope.to);
    if !to.is_empty() {
        headers.push(QuoteHeader {
            label: "To".to_owned(),
            value: to,
        });
    }
    let cc = display_addresses(&original.envelope.cc);
    if !cc.is_empty() {
        headers.push(QuoteHeader {
            label: "Cc".to_owned(),
            value: cc,
        });
    }
    headers.push(QuoteHeader {
        label: "Subject".to_owned(),
        value: original.envelope.subject.clone().unwrap_or_default(),
    });
    QuoteAttribution { line, headers }
}

/// One address as a header shows it: `Name <address>`, or the bare address when
/// the header carried no usable name.
pub(super) fn display_address(addr: &EmailAddress) -> String {
    match addr
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        Some(name) => format!("{name} <{}>", addr.email),
        None => addr.email.clone(),
    }
}

/// The addresses as a header shows them, comma-joined.
pub(super) fn display_addresses(addresses: &[EmailAddress]) -> String {
    addresses
        .iter()
        .map(display_address)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The instant as the attribution shows it: `9 Oct 2026 at 08:00`, the same
/// instant the list row and the reading pane order and stamp by (delivery date,
/// falling back to the `Date` header). `None` when neither is known.
pub(super) fn display_instant(instant: Option<UtcDateTime>) -> Option<String> {
    let instant = instant?;
    let month = MONTHS
        .get(usize::from(instant.month().saturating_sub(1)))
        .copied()
        .unwrap_or_default();
    Some(format!(
        "{} {month} {} at {:02}:{:02}",
        instant.day(),
        instant.year(),
        instant.hour(),
        instant.minute(),
    ))
}
