// SPDX-License-Identifier: GPL-3.0-only
//! Building the composition a reply or forward opens on: the Compose family's
//! arm, which reads the original's headers through the engine, computes the
//! prefill per mail-calendar's reply rules, and publishes it on the
//! [`Surface::ComposeRequest`](crate::Surface::ComposeRequest) snapshot the
//! composer hydrates from. The accepted smart-reply chip rides the same arm:
//! `App::compose_from_draft` publishes the reply's prefill with the drafted
//! reply already the document's opening paragraphs.
//!
//! The port source is mail-calendar's reply paths, reshaped one way: MC's
//! clients derive the prefill from what their reading views already hold, while
//! Astra's core computes it, so every surface opens the same composition the
//! send path would thread. The rules themselves port as they stand. Recipients:
//! the reply's `To` leads with the original's `Reply-To` (else `From`), a
//! reply-all preserves the original's layout with the account's own identity
//! left out, and a forward names no one. Subjects take `Re:`/`Fwd:` with the
//! original's own prefix preserved rather than doubled. The quote carries the
//! attribution line (the sender and the date, MC's format) and the original's
//! body as plain text: the plain part when there is one (the engine derives one
//! from an HTML-only message when it can), else the sanitised HTML read as
//! text. Rich nested-HTML quoting is a later slice's, recorded in the spec.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content, its
//! recipients, or its subject; ids and counts only.

use std::collections::HashSet;

use astra_composer::{
    Block, ComposerDocument, InlineContent, Paragraph, Quote, QuoteAttribution, QuoteHeader,
    QuoteStyle, TextRun,
};
use astra_viewmodel::{ComposeKind, ComposeRequestSnapshot};
use engine_api::{
    AccountId, EmailAddress, Message, MessageBody, MessageIdHeader, Provider, ProviderKey,
    UtcDateTime,
};

use crate::{reading::resolve, reference::MessageRef, sanitise, App, ComposeIntent};

// The html-to-text fallback a body with no derivable text rendering takes;
// split out so each file stays under the 500-line limit.
mod text;

// The agent draft seam: the compose request an assistant's create_draft
// opens, split out for the same limit.
mod agent_draft;

pub use agent_draft::OpenDraftError;

use text::to_plain;

/// The month names the attribution line's date spells out. The core has no
/// locale (the same rule that keeps the `Re:` prefix untranslated), so one
/// English spelling everywhere.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

impl<P: Provider> App<P> {
    /// The compose-request snapshot, for the host's pull after a
    /// [`Surface::ComposeRequest`](crate::Surface::ComposeRequest) signal: the
    /// composition the last reply or forward opened, or `None` when no request
    /// is pending.
    #[must_use]
    pub fn compose_request(&self) -> Option<ComposeRequestSnapshot> {
        self.compose_request.get()
    }

    /// Takes the pending compose request, consuming the slot: the pop-out
    /// composer window's pull, which hydrates from the answer and leaves the
    /// slot empty so no later window re-reads a spent request. A taken slot
    /// signals the surface, so every webview's pull re-reads the emptiness;
    /// an empty slot answers `None` without signalling. The compose source
    /// stays: the send path re-resolves the original from it to thread the
    /// reply, and it stands until the next request replaces it.
    pub fn take_compose_request(&self) -> Option<ComposeRequestSnapshot> {
        let pending = self.compose_request.get();
        if pending.is_some() {
            self.compose_request.publish(None);
        }
        pending
    }

    /// Routes one [`ComposeIntent`]: every variant builds and publishes one
    /// compose request from the message it names, so a host opening its
    /// composer on the signal pulls the whole prefill as one thing.
    pub(crate) async fn dispatch_compose(&self, intent: ComposeIntent) {
        let (message, kind) = match intent {
            ComposeIntent::Reply { message } => (message, ComposeKind::Reply),
            ComposeIntent::ReplyAll { message } => (message, ComposeKind::ReplyAll),
            ComposeIntent::Forward { message } => (message, ComposeKind::Forward),
        };
        self.publish_compose_request(&message, kind, None).await;
    }

    /// Publishes the reply composition one accepted smart-reply chip opens:
    /// the prefill a `Compose::Reply` builds with the drafted reply already
    /// in the document as its opening paragraphs, above the quoted original.
    /// The host's `compose_from_draft` command; the composer hydrates from
    /// the published snapshot exactly as it does from a reply's own dispatch.
    pub async fn compose_from_draft(&self, account: &AccountId, key: &ProviderKey, draft: &str) {
        let message = MessageRef {
            account: account.clone(),
            key: key.clone(),
        };
        self.publish_compose_request(&message, ComposeKind::Reply, Some(draft))
            .await;
    }

    /// Reads the original in its account, computes the prefill, and publishes
    /// it. A message that no longer resolves (moved, tombstoned, never synced)
    /// publishes nothing and says nothing: there is no honest composition to
    /// open, and an empty one would be a composer the user cannot trust. A
    /// body that cannot be fetched still publishes; the prefill needs only the
    /// headers, and the document opens on no quote rather than none at all.
    async fn publish_compose_request(
        &self,
        message: &MessageRef,
        kind: ComposeKind,
        draft: Option<&str>,
    ) {
        let Some(handle) = self.account_handle(&message.account).await else {
            return;
        };
        let Some(provider) = handle.providers.first() else {
            return;
        };
        let Some(original) = resolve(&self.engine, message).await else {
            log::warn!(
                "compose: account {} holds no such message; nothing to compose from",
                message.account.as_str(),
            );
            return;
        };
        let body = match self
            .engine
            .message_body(provider, &message.account, &original)
            .await
        {
            Ok(body) => Some(body),
            Err(err) => {
                log::warn!(
                    "compose: the body fetch failed for account {}: {err}",
                    message.account.as_str(),
                );
                None
            }
        };
        let snapshot = compose_request(
            kind,
            &message.account,
            &original,
            &handle.identity,
            body.as_ref(),
            draft,
        );
        // The submit arms re-resolve this message to thread the outgoing
        // reply or forward under the original's chain; the snapshot carries
        // no reference, so the original is held here beside it.
        *self
            .compose_source
            .lock()
            .expect("compose-source mutex poisoned") = Some(message.clone());
        self.compose_request.publish(Some(snapshot));
    }
}

/// Builds the snapshot one compose request publishes: the prefill for `kind`
/// computed from `original`'s headers, quoted body and all, composing from
/// `from` under the identity `identity` (whose address a reply-all leaves out).
/// `draft`, when a chip's acceptance carried one, opens the document on the
/// drafted reply's own paragraphs above the quote.
///
/// Pure, so the prefill rules are testable against a bare message the way the
/// reading pipeline's rendering is.
pub(crate) fn compose_request(
    kind: ComposeKind,
    from: &AccountId,
    original: &Message,
    identity: &EmailAddress,
    body: Option<&MessageBody>,
    draft: Option<&str>,
) -> ComposeRequestSnapshot {
    let (to, cc) = recipients(kind, original, identity);
    ComposeRequestSnapshot {
        kind,
        from: from.clone(),
        to,
        cc,
        subject: subject(kind, original.envelope.subject.as_deref()),
        in_reply_to: threading_header(kind, original),
        document: document(kind, original, body, draft),
    }
}

/// The recipients `kind` prefills, as `(to, cc)` comma-joined bare addresses.
///
/// The reply's `To` leads with every primary recipient: the original's
/// `Reply-To` when it has any, else its `From`. They go in unconditionally (a
/// reply must have a recipient) even when the sender is the user themselves
/// (replying to their own Sent copy), where self-exclusion would otherwise
/// leave the `To` empty and unsendable. Beyond the primary, a reply never
/// targets the user: the identity is excluded from everything below. A
/// reply-all preserves the original's layout (its other `To` recipients join
/// `To`, its `Cc` stays `Cc`); a plain reply leaves `Cc` to the user; a forward
/// names no one. Duplicates are removed case-insensitively while first-seen
/// order is kept, and `Bcc` is never derived: it is always the user's own
/// addition.
fn recipients(kind: ComposeKind, original: &Message, identity: &EmailAddress) -> (String, String) {
    if kind == ComposeKind::Forward {
        return (String::new(), String::new());
    }
    let mut seen: HashSet<String> = HashSet::new();
    let primary = if original.envelope.reply_to.is_empty() {
        &original.envelope.from
    } else {
        &original.envelope.reply_to
    };
    let mut to: Vec<&EmailAddress> = Vec::new();
    for addr in primary {
        push_unique(&mut seen, addr, &mut to);
    }
    seen.insert(identity.email.to_ascii_lowercase());
    let mut cc: Vec<&EmailAddress> = Vec::new();
    if kind == ComposeKind::ReplyAll {
        for addr in &original.envelope.to {
            push_unique(&mut seen, addr, &mut to);
        }
        for addr in &original.envelope.cc {
            push_unique(&mut seen, addr, &mut cc);
        }
    }
    (
        to.iter()
            .map(|addr| addr.email.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        cc.iter()
            .map(|addr| addr.email.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// Appends `addr` unless its lowercased address is already in `seen`: the
/// case-insensitive de-dup that keeps a reply-all from listing anyone twice (or
/// in both `To` and `Cc`) while preserving first-seen order.
fn push_unique<'a>(
    seen: &mut HashSet<String>,
    addr: &'a EmailAddress,
    into: &mut Vec<&'a EmailAddress>,
) {
    if seen.insert(addr.email.to_ascii_lowercase()) {
        into.push(addr);
    }
}

/// The subject `kind` prefills: the original's subject under `Re:` or `Fwd:`,
/// not doubled when it already carries the prefix.
fn subject(kind: ComposeKind, subject: Option<&str>) -> String {
    prefixed_subject(
        if kind == ComposeKind::Forward {
            "Fwd:"
        } else {
            "Re:"
        },
        subject,
    )
}

/// Prefixes `original` with `prefix` unless it already starts with it
/// (case-insensitively), so subjects do not accrete `Re: Re:`. The prefix is
/// not localised: it is what threads the conversation in every other mail
/// client, and a translated one groups nothing.
fn prefixed_subject(prefix: &str, original: Option<&str>) -> String {
    let subject = original.unwrap_or_default().trim();
    if subject
        .to_ascii_lowercase()
        .starts_with(&prefix.to_ascii_lowercase())
    {
        subject.to_owned()
    } else if subject.is_empty() {
        prefix.to_owned()
    } else {
        format!("{prefix} {subject}")
    }
}

/// The threading header `kind` carries: a reply answers the original's own
/// `Message-ID`; a forward carries none, because it continues a conversation
/// without answering a message. `None` too for an original that arrived with
/// no `Message-ID` at all, which is valid mail.
fn threading_header(kind: ComposeKind, original: &Message) -> Option<String> {
    if kind == ComposeKind::Forward {
        return None;
    }
    original
        .envelope
        .message_id
        .first()
        .map(MessageIdHeader::as_str)
        .map(str::to_owned)
}

/// The composer document `kind` opens on: an accepted draft's paragraphs
/// first, then the quoted original below them when there is one to quote.
/// With no draft the quote stands alone and the editor puts the user's own
/// paragraph above it; with neither there are no blocks, so a composer never
/// opens on an empty quote.
fn document(
    kind: ComposeKind,
    original: &Message,
    body: Option<&MessageBody>,
    draft: Option<&str>,
) -> ComposerDocument {
    let mut blocks = draft.map_or_else(Vec::new, draft_paragraphs);
    let quoted = quote_text(body);
    if !quoted.trim().is_empty() {
        blocks.push(Block::Quote(quote(kind, original, quoted)));
    }
    ComposerDocument {
        blocks,
        attachments: Vec::new(),
    }
}

/// The drafted reply as document blocks: one paragraph per line, each a
/// single unmarked run, a blank line an empty paragraph. The plain-text
/// resume path's own convention, so a multi-line draft opens as it would
/// from a stored copy.
fn draft_paragraphs(draft: &str) -> Vec<Block> {
    draft
        .lines()
        .map(|line| {
            let content = if line.is_empty() {
                Vec::new()
            } else {
                vec![InlineContent::Text(TextRun {
                    text: line.to_owned(),
                    bold: false,
                    italic: false,
                    underline: false,
                    font_size: None,
                    color: None,
                    highlight: None,
                    link: None,
                })]
            };
            Block::Paragraph(Paragraph { content })
        })
        .collect()
}

/// The quoted original: the default style (the account-preference setting is a
/// later slice's, recorded in the spec), the attribution above, and the body in
/// its two spellings.
fn quote(kind: ComposeKind, original: &Message, quoted: String) -> Quote {
    Quote {
        style: QuoteStyle::default(),
        attribution: attribution(kind, original),
        body_html: plain_html(&quoted),
        body_plain: quoted,
    }
}

/// The attribution above a quoted original, in both renderings a host can flip
/// between: the one-line a reply shows ("On `<date>`, `<sender>` wrote:", or
/// just the sender when the original carries no instant) and the labelled
/// header block a forward's style shows, which is the same for either kind.
fn attribution(kind: ComposeKind, original: &Message) -> QuoteAttribution {
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
fn display_address(addr: &EmailAddress) -> String {
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
fn display_addresses(addresses: &[EmailAddress]) -> String {
    addresses
        .iter()
        .map(display_address)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The instant as the attribution shows it: `9 Oct 2026 at 08:00`, the same
/// instant the list row and the reading pane order and stamp by (delivery date,
/// falling back to the `Date` header). `None` when neither is known.
fn display_instant(instant: Option<UtcDateTime>) -> Option<String> {
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

/// The text a quote carries: the body's plain part when it has one (the engine
/// derives one from an HTML-only message when it can), else the sanitised HTML
/// read as text. Plain-text quoting only, this slice: the rich nested-HTML
/// quote is the rich-paste slice's.
fn quote_text(body: Option<&MessageBody>) -> String {
    let Some(body) = body else {
        return String::new();
    };
    if let Some(plain) = body.plain().filter(|plain| !plain.trim().is_empty()) {
        return plain.to_owned();
    }
    body.html()
        .map_or_else(String::new, |html| to_plain(&sanitise::sanitise(html).html))
}

/// The quoted text as an inert HTML fragment: the text, HTML-escaped, inside a
/// `pre-wrap` element whose declaration is the only markup, so the newlines the
/// text carries are the quote's only structure and a rich renderer does not
/// collapse them onto one line. Inert by construction, which is what the
/// document's contract asks of a quote's html: nothing of the original's own
/// markup reaches it, only its words.
fn plain_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 48);
    out.push_str("<div style=\"white-space: pre-wrap;\">");
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out.push_str("</div>");
    out
}

#[cfg(test)]
#[path = "compose_request_tests.rs"]
mod tests;
