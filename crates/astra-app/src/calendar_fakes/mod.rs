// SPDX-License-Identifier: GPL-3.0-only
//! The calendar slice's fixtures: a provider that answers both the mail leg
//! (so an invitation message can live in the store) and the calendar leg (so
//! a sync, a write, and a reconcile all round-trip through the real engine
//! over an in-memory store), plus the diary and invitation builders every
//! calendar test starts from.
//!
//! The server-side copy the fake holds is **mutated by the writes it
//! accepts** the way a real server's is, so the post-write reconcile a facade
//! write runs presents the change and the store the snapshot reads holds what
//! the user did. A per-occurrence patch is recorded but not applied: the
//! occurrence-splitting surgery is the adapters' business, and the shape of
//! the recorded call is what the unit mapping tests pin.

#![allow(dead_code)]

mod fixtures;
mod provider;

pub(crate) use fixtures::{
    all_day_event, calendar, local, meeting, overridden_weekly_event, readonly, utc, weekly_event,
    zoned_event,
};

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use engine_api::{
    Calendar, Draft, Event, EventDeletion, EventDraft, EventEdit, EventRsvp, EventWrite, Mailbox,
    Message, RsvpResponse, ScheduleMethod,
};

use crate::{Account, App, AppObserver, Surface};

/// The raw bytes of an invitation the engine's own parser is proven on: a
/// `METHOD:REQUEST` body part, delivered to `info@test.local` (an alias), with
/// the organiser's and attendee's lines.
const INVITATION_SOURCE: &[u8] = b"From: boss@test.local\r\n\
    To: everyone@test.local\r\n\
    Delivered-To: info@test.local\r\n\
    Subject: Sprint planning\r\n\
    Message-ID: <invite-1@test.local>\r\n\
    Content-Type: multipart/alternative; boundary=\"a\"\r\n\r\n\
    --a\r\nContent-Type: text/plain\r\n\r\nWhen: 30 July\r\n\
    --a\r\nContent-Type: text/calendar; charset=\"utf-8\"; method=REQUEST\r\n\r\n\
    BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\n\
    UID:meeting-1@test.local\r\nDTSTAMP:20260701T080000Z\r\n\
    DTSTART;TZID=Europe/Berlin:20260730T163000\r\n\
    DTEND;TZID=Europe/Berlin:20260730T173000\r\n\
    SUMMARY:Sprint planning\r\nSEQUENCE:0\r\n\
    ORGANIZER;CN=Boss:mailto:boss@test.local\r\n\
    ATTENDEE;CN=Info;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:info@test.local\r\n\
    END:VEVENT\r\nEND:VCALENDAR\r\n\
    --a--\r\n";

/// The calendar-and-mail provider the calendar tests drive: a fixed tree and
/// diary served as an idempotent snapshot (so a reconcile presents the
/// server's current copy), writes recorded and applied to that copy, and a
/// scriptable count of event-sync refusals (the reconcile failure a
/// `FailedWithRetry` row reports).
pub(crate) struct CalendarFake {
    /// The account's mail tree, served on every folder-list pass.
    pub mailboxes: Vec<Mailbox>,
    /// The account's mailbox, served on every email pass.
    pub messages: Vec<Message>,
    /// Raw RFC 5322 sources by provider key, for the body and scheduling
    /// reads.
    pub sources: HashMap<String, Vec<u8>>,
    /// The account's calendars, served on every calendar-container pass.
    pub calendars: Vec<Calendar>,
    /// The server's diary, served on every event pass and mutated by the
    /// writes the fake accepts. Behind a mutex because the writes move it.
    pub events: Arc<Mutex<Vec<Event>>>,
    /// Events that join the diary when a whole-document write stores their
    /// `UID` (the invitation the app puts on the calendar itself), keyed by
    /// that `UID`.
    pub documents: HashMap<String, Event>,
    /// How many event passes to refuse ("server unreachable"), so a write's
    /// reconcile can be made to fail.
    pub fail_syncs: Arc<Mutex<u32>>,
    /// What the RSVP receipt reports about delivering the reply.
    pub reply_delivery: engine_api::ReplyDelivery,
    /// Whether the transport's calendar server schedules (the capability).
    pub schedules: bool,
    /// Whether the RSVP verb can carry a note for the organiser (the
    /// capability): `false` is the transport that must refuse one.
    pub rsvp_comment: bool,
    /// What a series-level edit does to the occurrences the user changed
    /// individually (the capability): the warning's server half.
    pub override_survival: engine_api::OverrideSurvival,
    /// How many calendar writes to refuse ("the write was rejected"), so an
    /// engine refusal can be scripted.
    pub fail_writes: Arc<Mutex<u32>>,
    /// How many submits to refuse ("the mail could not be sent"), so the iMIP
    /// reply's failure can be scripted.
    pub fail_submits: Arc<Mutex<u32>>,
    /// Every recorded create, in order.
    pub creates: Arc<Mutex<Vec<EventDraft>>>,
    /// Every recorded `(base, edit)` patch, in order.
    pub patches: Arc<Mutex<Vec<(Event, EventEdit)>>>,
    /// Every recorded `(base, rsvp)`, in order.
    pub rsvps: Arc<Mutex<Vec<(Event, EventRsvp)>>>,
    /// Every recorded `(base, deletion)` delete, in order.
    pub deletes: Arc<Mutex<DeleteLog>>,
    /// Every recorded whole-document write, in order.
    pub puts: Arc<Mutex<Vec<EventWrite>>>,
    /// Every submitted message, in order (the iMIP reply rides here).
    pub submits: Arc<Mutex<Vec<Draft>>>,
    /// While `Some` and closed, every event pass waits inside the fake, so a
    /// test keeps the account's event scope leased: the `Busy` an expansion
    /// or a second sync of that account then answers.
    pub event_gate: Option<Arc<EventGate>>,
}

/// Holds the fake's event pass inside `sync_events` while closed: the
/// account's event scope lease stays claimed, which is the scope contention
/// whose `Busy` a widening must roll back from.
pub(crate) struct EventGate {
    /// Whether passes wait.
    closed: AtomicBool,
    /// Signalled when one pass has entered its wait, so the holder knows the
    /// scope lease is claimed.
    entered: tokio::sync::Notify,
    /// Signalled to let a held pass through.
    open: tokio::sync::Notify,
}

impl EventGate {
    /// A gate that stands open: passes run through untouched.
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            closed: AtomicBool::new(false),
            entered: tokio::sync::Notify::new(),
            open: tokio::sync::Notify::new(),
        })
    }

    /// Closes the gate: event passes from now on hold inside the fake.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    /// Waits until one event pass holds, so the account's scope is leased.
    pub(crate) async fn until_held(&self) {
        self.entered.notified().await;
    }

    /// Lets a held pass through and leaves the gate open again.
    pub(crate) fn release(&self) {
        self.closed.store(false, Ordering::SeqCst);
        self.open.notify_one();
    }

    /// What one event pass runs: through untouched while open, a signalled
    /// wait while closed.
    async fn pass(&self) {
        if !self.closed.load(Ordering::SeqCst) {
            return;
        }
        self.entered.notify_one();
        self.open.notified().await;
    }
}

/// The delete recorder's log: one `(base, deletion)` pair per recorded call,
/// the base optional the way the engine's delete verb takes it.
pub(crate) type DeleteLog = Vec<(Option<Event>, EventDeletion)>;

impl CalendarFake {
    /// A provider holding one default writable calendar and the invitation
    /// message, with no events yet: the account shape every calendar test
    /// starts from.
    pub(crate) fn with_calendar_and_mail() -> Self {
        Self {
            mailboxes: vec![fixtures::inbox()],
            messages: vec![fixtures::invitation_message("invite-1")],
            sources: HashMap::from([("invite-1".to_owned(), INVITATION_SOURCE.to_vec())]),
            calendars: vec![fixtures::calendar("cal-1", "Work")],
            events: Arc::new(Mutex::new(Vec::new())),
            documents: HashMap::new(),
            fail_syncs: Arc::new(Mutex::new(0)),
            reply_delivery: engine_api::ReplyDelivery::NotReported,
            schedules: true,
            rsvp_comment: true,
            override_survival: engine_api::OverrideSurvival::kept(),
            fail_writes: Arc::new(Mutex::new(0)),
            fail_submits: Arc::new(Mutex::new(0)),
            creates: Arc::new(Mutex::new(Vec::new())),
            patches: Arc::new(Mutex::new(Vec::new())),
            rsvps: Arc::new(Mutex::new(Vec::new())),
            deletes: Arc::new(Mutex::new(Vec::new())),
            puts: Arc::new(Mutex::new(Vec::new())),
            submits: Arc::new(Mutex::new(Vec::new())),
            event_gate: None,
        }
    }

    /// Scripts the next `count` event passes to fail.
    pub(crate) fn failing_syncs(self, count: u32) -> Self {
        *self.fail_syncs.lock().unwrap() = count;
        self
    }

    /// Makes the server report it could not deliver an RSVP reply.
    pub(crate) fn failing_reply_delivery(mut self) -> Self {
        self.reply_delivery = engine_api::ReplyDelivery::Failed {
            status: "5.2".to_owned(),
        };
        self
    }

    /// Marks the transport as one whose calendar server does not schedule.
    pub(crate) fn without_scheduling(self) -> Self {
        Self {
            schedules: false,
            ..self
        }
    }

    /// Marks the RSVP verb as one with nowhere to carry a note.
    pub(crate) fn without_rsvp_notes(self) -> Self {
        Self {
            rsvp_comment: false,
            ..self
        }
    }

    /// Marks the transport as one whose series edits discard the occurrences
    /// the user changed individually: the measured Google shape, where the
    /// moves revert and the renames spread.
    pub(crate) fn discarding_overrides(self) -> Self {
        Self {
            override_survival: engine_api::OverrideSurvival {
                survives_time_change: false,
                survives_rule_change: false,
                clobbers_own_fields: true,
            },
            ..self
        }
    }

    /// Deposits `event` on the server's diary: the copy a server of its own
    /// put there.
    pub(crate) fn holding(self, event: Event) -> Self {
        self.events.lock().unwrap().push(event);
        self
    }

    /// Teaches the fake the event a whole-document write of `uid` stores.
    pub(crate) fn storing_document(mut self, uid: &str, event: Event) -> Self {
        self.documents.insert(uid.to_owned(), event);
        self
    }
}

/// The scheduling method of a draft's calendar part, if it carries one.
pub(crate) fn draft_method(draft: &Draft) -> Option<ScheduleMethod> {
    draft
        .calendar
        .as_ref()
        .map(|calendar| calendar.method.clone())
}

/// The words of a participation status the diary asserts on, so the tests can
/// name the RSVP outcome without the engine's open enum.
pub(crate) fn partstat_of(event: &Event, attendee: &str) -> String {
    event
        .participants
        .iter()
        .find(|participant| {
            participant
                .email
                .as_deref()
                .is_some_and(|address| engine_api::addresses_match(address, attendee))
        })
        .map(|participant| participant.participation_status.as_str().to_owned())
        .unwrap_or_default()
}

/// The response an RSVP recorded, as the engine's own three words.
pub(crate) fn response_word(rsvp: &EventRsvp) -> &'static str {
    match rsvp.response {
        RsvpResponse::Accepted => "accepted",
        RsvpResponse::Tentative => "tentative",
        RsvpResponse::Declined => "declined",
    }
}

/// Wraps `provider` as an account `id` whose identity is `info@test.local`:
/// the very address the invitation fixtures are delivered to, so the alias
/// story (matched attendee, participation, the diary) coheres on one address.
pub(crate) fn account(id: &str, provider: CalendarFake) -> Account<CalendarFake> {
    Account {
        id: engine_api::AccountId::try_from(id).unwrap(),
        providers: vec![provider],
        identity: engine_api::EmailAddress::new("info@test.local"),
    }
}

/// Builds an in-memory app over `accounts`, recording signalled surfaces.
pub(crate) fn calendar_app(
    accounts: Vec<Account<CalendarFake>>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> App<CalendarFake> {
    App::new(
        Arc::new(engine_api::Engine::open_in_memory().unwrap()),
        accounts,
        None,
        Arc::new(RecordingObserver {
            surfaces: Arc::clone(surfaces),
        }),
    )
}

/// Records the surfaces the app signals, so a test can assert the loop fired.
pub(crate) struct RecordingObserver {
    /// Every surface signalled so far, in order.
    pub surfaces: Arc<Mutex<Vec<Surface>>>,
}

impl AppObserver for RecordingObserver {
    fn surface_changed(&self, surface: Surface) {
        self.surfaces.lock().unwrap().push(surface);
    }
}
