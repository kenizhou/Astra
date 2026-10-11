// SPDX-License-Identifier: GPL-3.0-only
//! The iMIP leg: the iTIP `REPLY` document and the email that carries it, for
//! the account whose calendar server does not schedule. The engine ships no
//! `Event`-to-iTIP serializer (its projection is lossy in exactly the places a
//! scheduling message must be exact), so the reply is assembled here from
//! named fields, the same way the whole-document write's bytes are the
//! caller's.
//!
//! Two rules the document encodes, both RFC 5546's:
//!
//! - **The organiser and only the replying attendee** (§3.2.3): a reply
//!   carrying the full attendee list would leak every other invitee's answer
//!   to a server that never had it.
//! - **The reply comes from the party answering** (RFC 6047 §3): an
//!   organiser's scheduler that finds no `ATTENDEE` for the sender is entitled
//!   to ignore it, which on an aliased account is exactly what the primary
//!   address would produce.

use engine_api::{
    addresses_match, AccountId, Draft, DraftCalendar, EmailAddress, Event, InboundScheduling,
    Message, Provider, ScheduleMethod, UtcDateTime,
};

use crate::{
    calendar::{edit, now_utc},
    protocol::RsvpAnswer,
    App,
};
/// Everything the `METHOD:REPLY` document needs, resolved. Pure data: the
/// caller has already matched the alias and placed the meeting.
pub(crate) struct ImipReply<'a> {
    /// The meeting's `UID`, verbatim from the invitation.
    pub uid: &'a str,
    /// The `SEQUENCE` of the invitation being answered, so an organiser who
    /// has moved on can tell which revision this answers.
    pub sequence: u32,
    /// The organiser's calendar address, as the invitation wrote it.
    pub organizer: &'a str,
    /// The address answering, which on an aliased account is not the primary.
    pub attendee: &'a str,
    /// The attendee's display name, if the invitation carried one.
    pub attendee_name: Option<&'a str>,
    /// The answer.
    pub status: RsvpAnswer,
    /// The meeting's start as an instant: optional in a `REPLY` (RFC 5546
    /// §3.2.3) but helps a client that matches loosely.
    pub starts_at: UtcDateTime,
    /// The instance being answered, when the invitation named one. Load
    /// bearing: a reply that drops its `RECURRENCE-ID` answers the whole
    /// series.
    pub recurrence_id: Option<UtcDateTime>,
    /// When the reply was composed.
    pub stamp: UtcDateTime,
    /// The user's note, if any.
    pub comment: Option<&'a str>,
}

/// Builds the `METHOD:REPLY` iCalendar object: the organiser and **only the
/// replying attendee** (RFC 5546 §3.2.3), assembled from named fields rather
/// than derived by patching, because a reply carrying the full attendee list
/// would leak every other invitee's answer to a server that never had it. The
/// engine deliberately ships no `Event`-to-iTIP serializer (the projection is
/// lossy in exactly the places a scheduling message must be exact), so the
/// caller builds the text, the same way it builds the document a
/// whole-document write stores.
pub(crate) fn reply_document(reply: &ImipReply<'_>) -> String {
    let mut out = String::new();
    line(&mut out, "BEGIN:VCALENDAR");
    line(&mut out, "PRODID:-//Astra Mail//EN");
    line(&mut out, "VERSION:2.0");
    line(&mut out, "METHOD:REPLY");
    line(&mut out, "BEGIN:VEVENT");
    line(&mut out, &format!("UID:{}", escape_text(reply.uid)));
    line(&mut out, &format!("DTSTAMP:{}", stamp_text(reply.stamp)));
    line(
        &mut out,
        &format!("DTSTART:{}", stamp_text(reply.starts_at)),
    );
    if let Some(instance) = reply.recurrence_id {
        line(&mut out, &format!("RECURRENCE-ID:{}", stamp_text(instance)));
    }
    line(&mut out, &format!("SEQUENCE:{}", reply.sequence));
    line(
        &mut out,
        &format!("ORGANIZER:{}", cal_address(reply.organizer)),
    );
    line(&mut out, &attendee_line(reply));
    if let Some(comment) = reply.comment.map(str::trim).filter(|text| !text.is_empty()) {
        line(&mut out, &format!("COMMENT:{}", escape_text(comment)));
    }
    line(&mut out, "END:VEVENT");
    line(&mut out, "END:VCALENDAR");
    out
}

/// The one `ATTENDEE` line: the replier, their answer, and their name if the
/// invitation knew it. The engine spells a status the JSCalendar way
/// (lowercase); iCalendar `PARTSTAT` values are uppercase (RFC 5545
/// §3.2.12).
fn attendee_line(reply: &ImipReply<'_>) -> String {
    let mut out = String::from("ATTENDEE");
    if let Some(name) = reply.attendee_name.filter(|name| !name.trim().is_empty()) {
        out.push_str(";CN=\"");
        out.push_str(&name.replace('\\', "\\\\").replace('"', "\\\""));
        out.push('"');
    }
    out.push_str(";PARTSTAT=");
    out.push_str(match reply.status {
        RsvpAnswer::Accepted => "ACCEPTED",
        RsvpAnswer::Tentative => "TENTATIVE",
        RsvpAnswer::Declined => "DECLINED",
    });
    out.push(':');
    out.push_str(&cal_address(reply.attendee));
    out
}

/// An address as a `mailto:` URI, the form iTIP cal-addresses take.
fn cal_address(address: &str) -> String {
    let bare = address
        .strip_prefix("mailto:")
        .or_else(|| address.strip_prefix("MAILTO:"))
        .unwrap_or(address);
    format!("mailto:{bare}")
}

/// An instant as an iCalendar UTC `DATE-TIME` (RFC 5545 §3.3.5).
fn stamp_text(at: UtcDateTime) -> String {
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        at.year(),
        at.month(),
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    )
}

/// Escapes a text value per RFC 5545 §3.3.11: a backslash, a semicolon, a
/// comma, and a line break each carry a meaning in a content line, so each is
/// escaped rather than trusted.
fn escape_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
}

/// Appends one content line, folded to the 75-octet limit a strict parser is
/// entitled to enforce (RFC 5545 §3.1): a longer line continues with a single
/// leading space, which is how a `UID` long enough to overrun stays readable.
fn line(out: &mut String, content: &str) {
    let bytes = content.as_bytes();
    if bytes.len() <= 75 {
        out.push_str(content);
        out.push_str("\r\n");
        return;
    }
    let mut start = 0;
    let mut budget = 75;
    while start < bytes.len() {
        let end = (start + budget).min(bytes.len());
        let mut cut = end;
        while cut > start && !content.is_char_boundary(cut) {
            cut -= 1;
        }
        if start > 0 {
            out.push(' ');
        }
        out.push_str(&content[start..cut]);
        out.push_str("\r\n");
        start = cut;
        budget = 74;
    }
}

/// The display name the invitation gave this attendee, if it gave one.
pub(crate) fn attendee_name<'a>(event: &'a Event, attendee: &str) -> Option<&'a str> {
    event
        .participants
        .iter()
        .find(|participant| {
            participant
                .email
                .as_deref()
                .is_some_and(|address| addresses_match(address, attendee))
        })
        .and_then(|participant| participant.name.as_deref())
}

/// The reply's subject: the invitation's own, prefixed the RFC 5322 way. Not
/// an English "Accepted:", because the core has no locale and a reply that
/// announced the answer in a language the user does not speak would be worse
/// than one that quotes the meeting's own subject back; the answer itself is
/// in the iTIP part, which is what the organiser's client reads.
fn reply_subject(original: &Message) -> String {
    let subject = original.envelope.subject.as_deref().unwrap_or_default();
    if subject.is_empty() {
        "Re:".to_owned()
    } else if subject.len() >= 3 && subject[..3].eq_ignore_ascii_case("re:") {
        subject.to_owned()
    } else {
        format!("Re: {subject}")
    }
}

impl<P: Provider> App<P> {
    /// Builds the iTIP `REPLY` and mails it through the account's submit
    /// path: the step the organiser actually sees, on the route where nothing
    /// else would carry it. `subject_override` is the line the host composed
    /// when it composed one; absent, the reply quotes the invitation's own
    /// subject back.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_imip_reply(
        &self,
        provider: &P,
        account: &AccountId,
        original: &Message,
        scheduling: &InboundScheduling,
        attendee: &str,
        response: RsvpAnswer,
        comment: Option<&str>,
        subject_override: Option<&str>,
    ) {
        let event = &scheduling.message.event;
        let Some(organizer) = scheduling.message.organizer().map(str::to_owned) else {
            log::info!("calendar: the invitation names no organiser to reply to");
            return;
        };
        let zone = self.calendar_inputs().zone;
        let Ok(starts_at) = edit::instant_of(&event.start, &zone) else {
            log::info!("calendar: the meeting being answered cannot be placed");
            return;
        };
        let recurrence_id = match event
            .recurrence_id
            .as_ref()
            .map(|instance| edit::instant_of(instance, &zone))
        {
            Some(Ok(instant)) => Some(instant),
            Some(Err(_)) => {
                log::info!("calendar: the occurrence being answered cannot be placed");
                return;
            }
            None => None,
        };
        let document = reply_document(&ImipReply {
            uid: event.uid.as_str(),
            sequence: event.sequence,
            organizer: &organizer,
            attendee,
            attendee_name: attendee_name(event, attendee),
            status: response,
            starts_at,
            recurrence_id,
            stamp: now_utc().unwrap_or(starts_at),
            comment,
        });
        let subject = subject_override.map_or_else(|| reply_subject(original), str::to_owned);
        let body = match comment.map(str::trim).filter(|note| !note.is_empty()) {
            Some(note) => format!("{subject}\r\n\r\n{note}"),
            None => subject.clone(),
        };
        let Some(message_id) = crate::drafts::mint_message_id() else {
            log::warn!("calendar: the reply could not be given a message id");
            return;
        };
        let mut draft = Draft::new(
            message_id,
            // The reply comes from the party answering (RFC 6047 §3): an
            // organiser's scheduler that finds no `ATTENDEE` for the sender
            // is entitled to ignore it, which on an aliased account is
            // exactly what the primary address would produce.
            EmailAddress::new(attendee),
            vec![EmailAddress::new(
                organizer
                    .strip_prefix("mailto:")
                    .or_else(|| organizer.strip_prefix("MAILTO:"))
                    .unwrap_or(&organizer),
            )],
            subject,
            body,
        )
        .with_calendar(DraftCalendar::new(ScheduleMethod::Reply, document));
        if let Some(parent) = original.envelope.message_id.first() {
            draft = draft.in_reply_to(parent.clone(), vec![parent.clone()]);
        }
        let at = self.account_ordinal(account).await;
        match self.engine.submit_mail(provider, account, &draft).await {
            Ok(_) => log::info!("calendar[a{at}]: the iTIP reply was submitted"),
            Err(err) => {
                // The answer stands on the calendar; only the telling failed,
                // and a telling that silently went nowhere leaves the user
                // believing an organiser who has heard nothing.
                log::warn!("calendar[a{at}]: the iTIP reply could not be sent: {err}");
                self.mark_calendar_write(
                    account,
                    astra_viewmodel::CalendarWriteState::FailedWithRetry,
                );
                self.rebuild_calendar().await;
            }
        }
        self.drain_account(account).await;
        self.rebuild_sending().await;
    }
}

#[cfg(test)]
#[path = "../calendar_imip_tests.rs"]
mod tests;
