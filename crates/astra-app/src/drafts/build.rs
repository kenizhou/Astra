// SPDX-License-Identifier: GPL-3.0-only
//! Building the [`Draft`] a save stores, and the helpers around it: the recipient-field
//! split, the `Message-ID` a composition mints, the unchanged-content digest, and the
//! wall-clock instant the hint stamps.
//!
//! A child of [`super`], split out so that file stays under the 500-line limit.

use std::{
    hash::{DefaultHasher, Hasher},
    io,
};

use astra_composer::render;
use engine_api::{Draft, EmailAddress, MessageIdHeader};

use crate::list_rows::format_instant;
use engine_api::UtcDateTime;

use super::HeldSave;

/// Renders the composer's content into the draft to store: the same deterministic
/// renderer a send will use, so a stored draft and a sent message built from one
/// document are byte-identical. An empty recipient list is fine: a draft is unfinished
/// by definition.
pub(super) fn build_draft(
    message_id: &MessageIdHeader,
    identity: &EmailAddress,
    held: &HeldSave,
) -> Option<Draft> {
    let output = render(&held.document).ok()?;
    let mut draft = Draft::new(
        message_id.clone(),
        identity.clone(),
        parse_addresses(&held.to),
        held.subject.clone(),
        output.plain_text,
    )
    .with_cc(parse_addresses(&held.cc))
    .with_bcc(parse_addresses(&held.bcc));
    draft.html_body = Some(output.html);
    Some(draft)
}

/// A digest of everything a save would put on the server.
///
/// Over the serialized draft rather than a chosen subset of its fields: a subset is a
/// list that goes stale the moment the draft grows a field, and the failure is silent,
/// an edit the user made that never leaves the device. Streamed into the hasher rather
/// than buffered first, because JSON writes each byte as a decimal number.
pub(super) fn digest(draft: &Draft) -> u64 {
    let mut sink = HashSink(DefaultHasher::new());
    if serde_json::to_writer(&mut sink, draft).is_err() {
        return 0;
    }
    sink.0.finish()
}

/// An [`io::Write`] that hashes what is written to it and keeps none of it.
struct HashSink(DefaultHasher);

impl io::Write for HashSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Splits a comma-separated recipient field into addresses, dropping blanks.
///
/// Shared with the send path, which splits the composer's recipient fields the
/// same way a save does.
pub(crate) fn parse_addresses(field: &str) -> Vec<EmailAddress> {
    field
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(EmailAddress::new)
        .collect()
}

/// A fresh `Message-ID` for a composition's first save, from the wall clock: the outbox
/// keys on it for idempotency, so it has to be unique per message and stable across the
/// saves of one composition, which the process counter and the clock together give.
///
/// The fallback form keeps the counter, so even a mint whose primary form the header
/// rules refuse can never collide with another mint's. `None` only if both forms are
/// refused, which the caller treats as any other build failure rather than guessing an
/// id two compositions could share.
///
/// Also the mint a send uses: a send is its own message, idempotent by the same
/// header, unique per submit.
pub(crate) fn mint_message_id() -> Option<MessageIdHeader> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    MessageIdHeader::new(format!("draft-{nanos}-{seq}@astra.local"))
        .ok()
        .or_else(|| MessageIdHeader::new(format!("draft-{seq}@astra.local")).ok())
}

/// The wall clock as a whole-second UTC instant, the same shape the reading snapshot
/// stamps its messages with. The sending rows stamp their arrivals with it too.
pub(crate) fn now_instant() -> String {
    let now = time::OffsetDateTime::now_utc();
    UtcDateTime::new(
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
    .map_or_else(|_| String::new(), format_instant)
}
