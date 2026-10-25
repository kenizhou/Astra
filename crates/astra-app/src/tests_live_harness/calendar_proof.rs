// SPDX-License-Identifier: GPL-3.0-only
//! The shared fixtures of the gated calendar proofs: the run token, the
//! raw CalDAV seeding helper, the iCalendar event builder, the two-app
//! suite over one engine store (the mail app on the IMAP dial, the
//! calendar app on the real CalDAV adapter), the proof window, and the
//! snapshot readers the proofs assert through. Seeding speaks CalDAV
//! directly; every assertion goes through the product's own engine and
//! snapshot paths, never the raw helper. The diary proofs themselves live
//! in `calendar`, the invitation machine's in `calendar_invite`, and the
//! relayed meeting brief in `calendar_brief`, the same split the mail
//! suites made for the 500-line cap. The people proofs (`people`) ride
//! the same run token and the same raw request helper against the
//! listener's CardDAV face.

use std::{fmt::Write as _, sync::Arc, time::Duration};

use astra_viewmodel::CalendarSnapshot;
use engine_api::{AccountId, CalendarDateTime, Engine, Provider, TimeZoneId};
use provider_caldav::{CalDavConfig, CalDavProvider, Credentials};

use crate::{
    reference::{CalendarRef, EventRef},
    Account, App, AppObserver, CalendarIntent, Intent,
};

use super::{proof, CountingObserver};

pub(super) const DELIVERY_TIMEOUT: Duration = Duration::from_secs(15);

/// A unique token for one run: in every UID, title, and subject the run
/// mints, so reruns converge and cleanup matches only this run's artefacts.
pub(super) fn run_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("sp2h-{nanos}-{seq}")
}

/// The raw CalDAV seeding helper: one HTTP/1.0 request over a fresh
/// plaintext connection to the harness's shared HTTP listener, basic auth,
/// `Connection: close` so reading to EOF is the whole response. Seeding
/// only; no product assertion ever rides this.
pub(super) fn dav(method: &str, account: &str, uid: &str, password: &str, body: &str) -> u16 {
    let path = format!(
        "/dav/cal/{account}/default/{}.ics",
        encode_path_segment(uid)
    );
    dav_raw(method, &path, account, password, "text/calendar", body)
}

/// The raw DAV request under the seeding helpers: the same one-shot
/// plaintext request against an already-encoded `path` on the harness's
/// shared listener, with the content type the face expects. The people
/// proofs ride this against the CardDAV face beside the calendar's CalDAV
/// one; seeding and seeding-side cleanup only, never an assertion path.
pub(super) fn dav_raw(
    method: &str,
    path: &str,
    account: &str,
    password: &str,
    content_type: &str,
    body: &str,
) -> u16 {
    use std::io::{Read, Write};
    let base = super::jmap_base().expect("the gated run names the harness origin");
    let authority = base.strip_prefix("http://").unwrap_or(&base).to_owned();
    let request = format!(
        "{method} {path} HTTP/1.0\r\nHost: {authority}\r\nAuthorization: Basic {}\r\n\
         Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        base64(&format!("{account}:{password}")),
        body.len(),
    );
    let mut stream = std::net::TcpStream::connect(&authority).expect("the harness HTTP listener");
    stream
        .write_all(request.as_bytes())
        .expect("the seeding request writes");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .expect("the seeding answer");
    let head = String::from_utf8_lossy(&response);
    head.split(' ')
        .nth(1)
        .and_then(|code| code.trim().parse().ok())
        .unwrap_or_else(|| panic!("the seeding answer is not HTTP: {head:?}"))
}

/// Percent-encodes `segment` as one URI path segment (RFC 3986 §2.3): a
/// `UID` is opaque, and nothing forbids a slash.
pub(super) fn encode_path_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for &byte in segment.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Standard base64 (RFC 4648) of `text`: the one encoding HTTP basic auth
/// reads, small enough to live here rather than pull a dependency in.
fn base64(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut buffer = [0u8; 3];
        buffer[..chunk.len()].copy_from_slice(chunk);
        let word = u32::from(buffer[0]) << 16 | u32::from(buffer[1]) << 8 | u32::from(buffer[2]);
        out.push(char::from(ALPHABET[(word >> 18) as usize & 0x3f]));
        out.push(char::from(ALPHABET[(word >> 12) as usize & 0x3f]));
        if chunk.len() > 1 {
            out.push(char::from(ALPHABET[(word >> 6) as usize & 0x3f]));
        }
        if chunk.len() > 2 {
            out.push(char::from(ALPHABET[word as usize & 0x3f]));
        }
    }
    out
}

/// One seeded event's iCalendar bytes: a `VEVENT` under `summary`, with
/// full `DTSTART` and `DTEND` content lines (the all-day `VALUE=DATE`
/// parameter included where the caller wants it) and the extra lines the
/// caller names.
pub(super) fn event_ics(
    uid: &str,
    summary: &str,
    dtstart: &str,
    dtend: &str,
    extra: &[&str],
) -> String {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Astra harness//EN\r\n");
    out.push_str("BEGIN:VEVENT\r\n");
    let _ = write!(out, "UID:{uid}\r\n");
    out.push_str("DTSTAMP:20261011T000000Z\r\n");
    let _ = write!(out, "{dtstart}\r\n");
    let _ = write!(out, "{dtend}\r\n");
    let _ = write!(out, "SUMMARY:{summary}\r\n");
    for line in extra {
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");
    out
}

/// One iTIP `REQUEST`, the scheduling object an invitation mail carries: the
/// plain event plus the `VCALENDAR`-level `METHOD` the engine's parser
/// requires (an object without one is not a scheduling object) and the
/// organiser and attendee lines Stalwart's scheduling and the product's
/// two-condition gate both read.
pub(super) fn request_ics(
    uid: &str,
    summary: &str,
    dtstart: &str,
    dtend: &str,
    organizer: &str,
    attendee: &str,
) -> String {
    let mut out = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Astra harness//EN\r\nMETHOD:REQUEST\r\n",
    );
    out.push_str("BEGIN:VEVENT\r\n");
    let _ = write!(out, "UID:{uid}\r\n");
    out.push_str("DTSTAMP:20261011T000000Z\r\n");
    let _ = write!(out, "{dtstart}\r\n");
    let _ = write!(out, "{dtend}\r\n");
    let _ = write!(out, "SUMMARY:{summary}\r\n");
    let _ = write!(out, "ORGANIZER:mailto:{organizer}\r\n");
    let _ = write!(out, "ATTENDEE;RSVP=TRUE:mailto:{attendee}\r\n");
    out.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");
    out
}

/// The suite's app pair over one shared engine store: the mail app (the
/// IMAP dial the delivery proofs ride) and the calendar app (the real
/// CalDAV adapter over plain HTTP), each holding both harness accounts
/// under the ids the mail dial pinned. Returns the pair with astra1's and
/// astra2's account ids.
pub(super) async fn suite_apps(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (
    App<Box<dyn Provider>>,
    App<Box<dyn Provider>>,
    AccountId,
    AccountId,
) {
    let connect = |login: String| async move {
        CalDavProvider::connect(CalDavConfig::new(
            super::jmap_base().expect("the gated run names the harness origin"),
            Credentials::Basic {
                username: login,
                password: password.to_owned(),
            },
        ))
        .await
        .expect("the CalDAV dial and discovery")
    };
    let cal_first = connect(user.to_owned()).await;
    let cal_second = connect(proof::RECIPIENT.to_owned()).await;
    let (first, mail_first, ..) =
        super::dial_with_accepted_certificate(host, port, user, password).await;
    let (second, mail_second, ..) =
        super::dial_with_accepted_certificate(host, port, proof::RECIPIENT, password).await;
    let engine = Arc::new(Engine::open_in_memory().expect("the engine store"));
    let observer = || Arc::new(CountingObserver::default()) as Arc<dyn AppObserver>;
    let mail = App::new(
        Arc::clone(&engine),
        vec![
            Account {
                id: first.clone(),
                providers: mail_first,
                identity: engine_api::EmailAddress::new(user),
            },
            Account {
                id: second.clone(),
                providers: mail_second,
                identity: engine_api::EmailAddress::new(proof::RECIPIENT),
            },
        ],
        None,
        observer(),
    );
    let calendar = App::new(
        engine,
        vec![
            Account {
                id: first.clone(),
                providers: vec![Box::new(cal_first) as Box<dyn Provider>],
                identity: engine_api::EmailAddress::new(user),
            },
            Account {
                id: second.clone(),
                providers: vec![Box::new(cal_second) as Box<dyn Provider>],
                identity: engine_api::EmailAddress::new(proof::RECIPIENT),
            },
        ],
        None,
        observer(),
    );
    (mail, calendar, first, second)
}

/// The suite's fixed proof window: Monday 2 November 2026, three weeks
/// wide so a weekly series renders three times inside it.
pub(super) fn range() -> Intent {
    Intent::Calendar(CalendarIntent::SetRange {
        start: "2026-11-02T00:00:00Z".parse().expect("the window start"),
        days: 21,
        view: None,
    })
}

/// A UTC-zoned event time, the form every test's creates state.
pub(super) fn zoned(value: &str) -> CalendarDateTime {
    CalendarDateTime::Zoned {
        local: value.parse().expect("the wall clock"),
        zone: TimeZoneId::utc(),
    }
}

/// The snapshot's occurrence rows whose title carries `needle`.
pub(super) fn rows_of<'a>(
    snapshot: &'a CalendarSnapshot,
    needle: &str,
) -> Vec<&'a astra_viewmodel::OccurrenceRow> {
    snapshot
        .occurrences
        .iter()
        .filter(|row| row.title.contains(needle))
        .collect()
}

/// The writable default calendar of `account`, as the create intent names it.
pub(super) fn default_calendar(app: &App<Box<dyn Provider>>, account: &AccountId) -> CalendarRef {
    let snapshot = app.calendar();
    let row = snapshot
        .calendars
        .iter()
        .find(|calendar| calendar.account == *account && calendar.can_write)
        .expect("the account lists a writable calendar");
    CalendarRef::from_parts(account.as_str(), row.id.key().as_str().to_owned())
        .expect("the calendar reference")
}

/// The event reference of the first row matching `needle` on `account`.
pub(super) fn event_of_account(
    app: &App<Box<dyn Provider>>,
    account: &AccountId,
    needle: &str,
) -> EventRef {
    let snapshot = app.calendar();
    let row = rows_of(&snapshot, needle)
        .iter()
        .find(|row| row.account == *account)
        .expect("a row of the account names the event")
        .to_owned();
    EventRef::from_parts(row.account.as_str(), row.event.as_str().to_owned())
        .expect("the event reference")
}

/// The bounded wait: polls with refreshes until `account` itself holds rows
/// titled with `needle`, within the delivery timeout. Scoped to the one
/// account because a needle can render on another account first (the
/// organiser's own copy of a meeting the attendee's deposit is still
/// travelling to); the wait polls the real state, never a fixed sleep.
pub(super) async fn until_account_rows(
    app: &App<Box<dyn Provider>>,
    account: &AccountId,
    needle: &str,
) {
    let deadline = std::time::Instant::now() + DELIVERY_TIMEOUT;
    loop {
        app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
            .await
            .unwrap();
        if rows_of(&app.calendar(), needle)
            .iter()
            .any(|row| row.account == *account)
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no occurrence titled with {needle} landed on the account within the delivery timeout"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
