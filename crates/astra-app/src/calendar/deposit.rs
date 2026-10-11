// SPDX-License-Identifier: GPL-3.0-only
//! The stored-copy leg of the invitation path: finding the meeting a message
//! carries on the account's calendar, and putting it there when it never left
//! the mailbox. No RFC moves an invitation into the calendar, so the client
//! stores it (RFC 6638 §3.2.2), from the invitation's own bytes, under a
//! guarded create whose precondition turns a copy the server already deposited
//! into a find rather than a duplicate.

use engine_api::{AccountId, Event, EventId, EventWrite, InboundScheduling, Provider, RawIcal};

use crate::{calendar::writes::generated_idempotency, App};

impl<P: Provider> App<P> {
    /// The stored copy of the invitation's meeting, found through the
    /// materialized occurrences over the meeting's own span (never a decode of
    /// the account's whole diary) and matched by `UID`.
    pub(crate) async fn stored_event_by_uid(
        &self,
        account: &AccountId,
        event: &Event,
    ) -> Option<Event> {
        let zone = self.calendar_inputs().zone;
        let starts_at = super::edit::instant_of(&event.start, &zone).ok()?;
        let span = core::time::Duration::new(
            event
                .duration
                .days()
                .saturating_mul(86_400)
                .saturating_add(event.duration.seconds()),
            event.duration.nanoseconds(),
        );
        let window = engine_api::Horizon::new(starts_at, starts_at.checked_add(span)?).ok()?;
        let rows = self.engine.occurrences_in(account, window).await.ok()?;
        let keys: Vec<engine_api::ProviderKey> = rows
            .into_iter()
            .map(|row| row.event)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        self.engine
            .events_by_keys(account, &keys)
            .await
            .ok()?
            .into_iter()
            .find(|stored| stored.uid == event.uid)
    }

    /// Puts the invitation on the account's calendar from its own bytes, under
    /// a guarded create (`IfAbsent`): a resource already at the target is the
    /// server's own copy of the same meeting, found rather than overwritten.
    /// `None` when the account has no calendar, no stored raw bytes survived
    /// the parse, or the write was refused outright.
    pub(crate) async fn store_invitation(
        &self,
        provider: &P,
        account: &AccountId,
        scheduling: &InboundScheduling,
    ) -> Option<Event> {
        let event = &scheduling.message.event;
        let raw = event.raw_ical.as_ref()?;
        let document = for_storage(raw.as_str())?;
        let mut calendars = self.engine.calendars(account).await.ok()?;
        if calendars.is_empty() {
            // An empty list from the store means *not looked yet*, not "no
            // calendars": answering must not require a visit to the calendar
            // tab first, so the containers sync once here.
            let zone = self.calendar_inputs().zone;
            let window = self.sync_horizon();
            let _ = self
                .engine
                .sync_calendar(provider, account, window, &zone)
                .await;
            calendars = self.engine.calendars(account).await.ok()?;
        }
        let collection = calendars.first()?;
        let href = event_href(collection.id.key().as_str(), event.uid.as_str())?;
        let key = href.key().clone();
        let write = EventWrite::creating(href, event.uid.clone(), RawIcal::new(document));
        self.mark_calendar_write(account, astra_viewmodel::CalendarWriteState::Writing);
        self.rebuild_calendar().await;
        match self
            .engine
            .put_calendar_document(provider, account, &generated_idempotency(), &write)
            .await
        {
            Ok(landed) => {
                log::info!("calendar: the invitation was stored on the calendar");
                self.settle_calendar_write(account, &landed.reconciled)
                    .await;
            }
            Err(err) if err.is_conflict() => {
                log::info!("calendar: the meeting was already on the calendar; using it");
                self.clear_calendar_write(account);
                let _ = self
                    .engine
                    .reconcile_calendar_events(provider, account)
                    .await;
            }
            Err(err) => {
                log::info!("calendar: the meeting could not be stored: {err}");
                self.mark_calendar_write(
                    account,
                    astra_viewmodel::CalendarWriteState::FailedWithRetry,
                );
                self.rebuild_calendar().await;
                return None;
            }
        }
        let written = self
            .engine
            .events_by_keys(account, std::slice::from_ref(&key))
            .await
            .ok()?
            .into_iter()
            .next();
        match written {
            Some(event) => Some(event),
            // A server whose href convention is not ours put the copy
            // somewhere of its own choosing; find it by UID over the
            // meeting's span instead, within whatever window the store has
            // materialized.
            None => self.stored_event_by_uid(account, event).await,
        }
    }
}

/// Strips the `METHOD` property from a calendar object so the same bytes can
/// be **stored**: RFC 4791 §4.1 forbids `METHOD` on a calendar object
/// resource, because it describes a message in transit, not a meeting.
/// Everything else survives byte for byte, the `VTIMEZONE`, the `X-`
/// properties, and the `ATTENDEE` lines that are the whole point of storing
/// an invitation rather than a plain appointment.
fn for_storage(raw: &str) -> Option<String> {
    if !raw.contains("BEGIN:VCALENDAR") {
        return None;
    }
    // One trailing newline is stripped and re-added rather than split away,
    // so a source that ends without one, or with a blank line, crosses byte
    // for byte rather than gaining or losing an empty line at the end.
    let body = raw.strip_suffix("\r\n").unwrap_or(raw);
    let mut out = String::new();
    for content in body.split("\r\n") {
        if content
            .trim_end()
            .to_ascii_uppercase()
            .starts_with("METHOD:")
        {
            continue;
        }
        out.push_str(content);
        out.push_str("\r\n");
    }
    Some(out)
}

/// Mints the resource address a stored invitation lands at:
/// `<collection><uid>.ics`, the universal convention, which is what makes the
/// guarded create's precondition worth having: a server that deposited its
/// own copy of this meeting put it at the same address, so the guard finds it
/// instead of duplicating it. A random name would always succeed and always
/// duplicate.
fn event_href(collection: &str, uid: &str) -> Option<EventId> {
    let mut href = collection.to_owned();
    if !href.ends_with('/') {
        href.push('/');
    }
    href.push_str(&encode_path_segment(uid));
    href.push_str(".ics");
    EventId::try_from(href.as_str()).ok()
}

/// Percent-encodes `segment` as a single URI path segment (RFC 3986 §2.3): a
/// `UID` is opaque, and nothing forbids a slash.
fn encode_path_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for &byte in segment.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(
                char::from_digit(u32::from(byte >> 4), 16)
                    .unwrap_or('0')
                    .to_ascii_uppercase(),
            );
            out.push(
                char::from_digit(u32::from(byte & 0x0f), 16)
                    .unwrap_or('0')
                    .to_ascii_uppercase(),
            );
        }
    }
    out
}

#[cfg(test)]
#[path = "../calendar_deposit_tests.rs"]
mod tests;
