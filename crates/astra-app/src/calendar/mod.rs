// SPDX-License-Identifier: GPL-3.0-only
//! The calendar half of the runtime: the [`CalendarIntent`] dispatch over the
//! engine's calendar reads and writes, the view state a rebuild projects, and
//! the write tail every calendar write shares.
//!
//! The engine owns calendars, occurrence expansion, persistence, and reply
//! submission; this module owns what the intents mean as product behaviour:
//! the window the grids show (widened through
//! [`Engine::expand_horizon`](engine_api::Engine::expand_horizon), because
//! re-syncing never materializes a window nothing has expanded), the visible
//! calendar set, the write states (a write that reconciles to anything but
//! `Applied` **landed**; its recovery is a re-read on the next refresh, never
//! the same write issued again), and the one-vs-all occurrence question the
//! intents never guess at.

mod brief;
mod brief_drive;
mod deposit;
mod drag;
mod edit;
mod imip;
mod invite;
mod prompt;
mod scope;
mod snapshot;
mod writes;

use std::collections::{HashMap, HashSet};

use astra_viewmodel::{CalendarWriteRow, CalendarWriteState, Participation, ReplyPromptRow};
use engine_api::{AccountId, Horizon, Provider, TimeZoneId, UtcDateTime};

use crate::calendar::prompt::ReplyFallback;
use crate::{reference::CalendarRef, App, CalendarIntent};

/// The account's calendar provider: the first of its providers whose
/// connection advertises the calendar capability, wherever in the list it
/// stands (a dav dial appends it after the mail providers). `None` for an
/// account with none, whose calendar paths step aside exactly as they did
/// before any provider could speak the domain.
pub(crate) fn calendar_provider_of<P: Provider>(handle: &crate::Account<P>) -> Option<&P> {
    handle
        .providers
        .iter()
        .find(|provider| provider.connection_info().capabilities.calendars())
}

pub use brief::{
    generate_meeting_brief, meeting_brief_prompt, BriefDocumentError, BriefMeeting,
    BriefMeetingRow, MeetingBriefError, MeetingBriefState, MeetingBriefView,
    BRIEF_INVITATION_SCAN_LIMIT, MEETING_BRIEF_PROMPT_VERSION,
};

// The invitation gate, shared with the reading pipeline so the card above the
// open message's body and the answer path decide through one verdict.
pub(crate) use invite::{classify, matched_attendee, supersede, InvitationKind};

/// The visible window's default length, in days: the six weeks a month grid
/// needs, with a week of history behind now so a cold app that has never seen
/// a `SetRange` still paints the week it is in.
const DEFAULT_WINDOW_DAYS: u16 = 42;

/// How far ahead of the window the horizon widens when it must, in days: the
/// coarse chunk of the survey's contract. Expansion re-derives every stored
/// event, so it moves in month-sized steps against the watermark rather than
/// once per page.
const HORIZON_CHUNK_DAYS: u64 = 31;

/// The calendar view state: what a rebuild projects and the intents move. Held
/// in one place behind one mutex because the rebuild reads all of it and every
/// intent writes exactly one field of it.
#[derive(Default)]
pub(crate) struct CalendarState {
    /// The visible window, or `None` until the first `SetRange`: then the
    /// default window around the build instant.
    window: Option<Window>,
    /// Which calendars' events are drawn, or `None` until the first
    /// `SetVisibleCalendars`: then every calendar's. `Some` of an empty set is
    /// the user hiding everything, a state the rebuild must be able to hold.
    visible: Option<HashSet<(AccountId, String)>>,
    /// The grids' navigation anchor (`SetDate`), for the surfaces that ask
    /// where the view is centred.
    anchor: Option<engine_api::CalendarDate>,
    /// The horizon the app has widened to, per account shared: what
    /// `expand_horizon` has covered, so a window inside it costs nothing and a
    /// window outside it widens in one coarse step.
    horizon: Option<(UtcDateTime, UtcDateTime)>,
    /// Each account's write state, for the snapshot's write rows. Absent
    /// means no write is in flight and none is unconfirmed.
    writes: HashMap<AccountId, CalendarWriteState>,
    /// The display zone's IANA id. `Etc/UTC` (the spelling the engine's zone
    /// resolver round-trips) until a zone intent exists (the family carries
    /// none), which every rebuild resolves floating and all-day rows through
    /// and the horizon expands under.
    zone: String,
    /// The answers whose reply the server reported it could not deliver.
    prompts: Vec<ReplyPrompt>,
    /// Each account's standing answer to the reply-delivery-failure prompt:
    /// whether to email the organiser ourselves when the account's calendar
    /// server cannot deliver a reply. Absent means ask, every time, because
    /// sending mail as the user is the user's decision to make once per
    /// server, not once for ever.
    reply_fallbacks: HashMap<AccountId, ReplyFallback>,
}

impl CalendarState {
    /// The state every app starts with: no window, no visible set, no anchor,
    /// no watermark, no writes, and the UTC display zone.
    pub(crate) fn new() -> Self {
        Self {
            zone: "Etc/UTC".to_owned(),
            ..Self::default()
        }
    }
}

/// The window a rebuild or a refresh falls back to before the first
/// `SetRange`: six weeks around `now`, a week of them behind, so a cold app
/// still paints the week it is in. One definition, because a refresh that
/// synced a different span than the one the rebuild reads would materialize
/// occurrences the grid never shows, and show ones it never synced.
pub(crate) fn default_window(now: UtcDateTime) -> Window {
    Window {
        start: now
            .checked_sub(core::time::Duration::from_hours(7 * 24))
            .unwrap_or(now),
        days: DEFAULT_WINDOW_DAYS,
    }
}

/// The visible window the grids lay out over.
#[derive(Clone, Copy)]
pub(crate) struct Window {
    /// The UTC instant the window opens at.
    start: UtcDateTime,
    /// How many days the window spans.
    days: u16,
}

impl Window {
    /// The half-open UTC span the window covers.
    fn horizon(self) -> Horizon {
        let span = core::time::Duration::from_secs(u64::from(self.days) * 86_400);
        Horizon::new(
            self.start,
            self.start.checked_add(span).unwrap_or(self.start),
        )
        .unwrap_or_else(|_| {
            // A horizon refuses an empty range; a zero-day window is the only
            // way to ask for one, and clamping to one second keeps every read
            // valid without inventing days the host never asked to see.
            Horizon::new(
                self.start,
                self.start
                    .checked_add(core::time::Duration::from_secs(1))
                    .expect("one second is representable"),
            )
            .expect("a one-second window is never empty")
        })
    }
}

/// One answer the calendar server reported it could not deliver, held for the
/// snapshot's prompt row.
#[derive(Clone)]
pub(crate) struct ReplyPrompt {
    /// The account whose server failed to deliver.
    pub account: AccountId,
    /// The invitation message, by its provider key.
    pub message: engine_api::ProviderKey,
    /// The meeting's title.
    pub summary: String,
    /// The organiser's address.
    pub organizer: String,
    /// The answer that was given.
    pub response: Participation,
    /// The note the user wrote, if any.
    pub comment: Option<String>,
    /// The transport's own status token.
    pub status_code: String,
}

/// What one rebuild reads: the decision inputs, cloned out of the state so no
/// lock is held across an engine read.
pub(crate) struct CalendarInputs {
    /// The window the occurrence read covers.
    pub window: Horizon,
    /// The display zone floating and all-day rows resolve through.
    pub zone: TimeZoneId,
    /// The visible set, or `None` for every calendar.
    visible: Option<HashSet<(AccountId, String)>>,
    /// The write rows, one per account with a state.
    pub writes: Vec<CalendarWriteRow>,
    /// The reply-delivery-failure prompts.
    pub prompts: Vec<ReplyPromptRow>,
}

impl CalendarInputs {
    /// Whether `calendar` of `account` is drawn.
    pub(crate) fn is_visible(
        &self,
        account: &AccountId,
        calendar: &engine_api::CalendarId,
    ) -> bool {
        self.visible
            .as_ref()
            .is_none_or(|set| set.contains(&(account.clone(), calendar.key().as_str().to_owned())))
    }
}

impl<P: Provider> App<P> {
    /// The calendar snapshot, for the host's pull after a
    /// [`Surface::Calendar`](crate::Surface::Calendar) signal.
    #[must_use]
    pub fn calendar(&self) -> astra_viewmodel::CalendarSnapshot {
        self.calendar.get()
    }

    /// The decision inputs for one rebuild, and the default window the rebuild
    /// falls back to when no `SetRange` has landed: six weeks around the build
    /// instant, a week of it behind now.
    fn calendar_inputs(&self) -> CalendarInputs {
        let state = self.calendar_state.lock().expect("calendar mutex poisoned");
        let now = now_utc().unwrap_or_else(|| {
            UtcDateTime::new(2026, 1, 1, 0, 0, 0).expect("a representable instant")
        });
        let window = state.window.unwrap_or_else(|| default_window(now));
        CalendarInputs {
            window: window.horizon(),
            zone: TimeZoneId::iana(&state.zone).unwrap_or_else(|_| TimeZoneId::utc()),
            visible: state.visible.clone(),
            writes: {
                let mut rows: Vec<CalendarWriteRow> = state
                    .writes
                    .iter()
                    .map(|(account, state)| CalendarWriteRow {
                        account: account.clone(),
                        state: *state,
                    })
                    .collect();
                rows.sort_by(|a, b| a.account.as_str().cmp(b.account.as_str()));
                rows
            },
            prompts: state
                .prompts
                .iter()
                .map(|prompt| ReplyPromptRow {
                    account: prompt.account.clone(),
                    message: prompt.message.clone(),
                    summary: prompt.summary.clone(),
                    organizer: prompt.organizer.clone(),
                    response: prompt.response,
                    comment: prompt.comment.clone(),
                    status_code: prompt.status_code.clone(),
                })
                .collect(),
        }
    }

    /// Routes one [`CalendarIntent`] to its handler. Every arm publishes the
    /// calendar surface on completion, because every arm moves something the
    /// snapshot shows (the window, the visible set, a write's outcome) or the
    /// now line that rides along with it.
    pub(crate) async fn dispatch_calendar(&self, intent: CalendarIntent) {
        match intent {
            CalendarIntent::Refresh => self.refresh_calendar().await,
            CalendarIntent::SetRange { start, days } => {
                self.set_calendar_window(start, days).await;
            }
            CalendarIntent::SetVisibleCalendars { ids } => {
                self.set_visible_calendars(&ids).await;
            }
            CalendarIntent::CreateEvent {
                calendar,
                summary,
                start,
                end,
                description,
                location,
                recurrence,
            } => {
                self.create_calendar_event(
                    &calendar,
                    &summary,
                    &start,
                    &end,
                    description.as_deref(),
                    location.as_deref(),
                    recurrence.as_ref(),
                )
                .await;
            }
            CalendarIntent::UpdateEvent { event, edit } => {
                self.update_calendar_event(&event, &edit).await;
            }
            CalendarIntent::MoveEvent { event, drag } => {
                self.move_calendar_event(&event, &drag).await;
            }
            CalendarIntent::DeleteEvent { event, occurrence } => {
                self.delete_calendar_event(&event, occurrence).await;
            }
            CalendarIntent::RespondToInvitation {
                message,
                response,
                comment,
                notify_organizer,
            } => {
                self.respond_to_invitation(message, response, comment, notify_organizer)
                    .await;
            }
            CalendarIntent::AnswerReplyPrompt {
                send,
                remember,
                reply_subject,
            } => {
                self.answer_reply_prompt(send, remember, reply_subject)
                    .await;
            }
            CalendarIntent::SetDate { date } => {
                self.calendar_state
                    .lock()
                    .expect("calendar mutex poisoned")
                    .anchor = Some(date);
                self.rebuild_calendar().await;
            }
        }
    }

    /// Syncs every account's calendars and events over the widened horizon,
    /// then rebuilds. This is also the recovery a `FailedWithRetry` row waits
    /// for: a successful re-read is the one thing that resolves an
    /// unconfirmed-but-landed write, and a refresh that lands one clears the
    /// row for the account it re-read.
    async fn refresh_calendar(&self) {
        let zone = self.calendar_inputs().zone;
        let horizon = self.sync_horizon();
        for handle in self.account_handles().await {
            let Some(provider) = calendar_provider_of(&handle) else {
                continue;
            };
            let at = self.account_ordinal(&handle.id).await;
            match self
                .engine
                .sync_calendar(provider, &handle.id, horizon, &zone)
                .await
            {
                Ok(_) => {
                    self.clear_calendar_write(&handle.id);
                    log::info!("calendar[a{at}]: the calendars synced");
                }
                Err(engine_api::ApiError::Busy) => {
                    log::info!("calendar[a{at}]: another sync holds the calendar; skipped");
                }
                Err(err) => log::warn!("calendar[a{at}]: the calendar sync failed: {err}"),
            }
        }
        self.rebuild_calendar().await;
    }

    /// Moves the visible window, widens the horizon to cover it when it has
    /// escaped the watermark, and rebuilds.
    async fn set_calendar_window(&self, start: UtcDateTime, days: u16) {
        self.calendar_state
            .lock()
            .expect("calendar mutex poisoned")
            .window = Some(Window { start, days });
        self.ensure_horizon().await;
        self.rebuild_calendar().await;
    }

    /// Records the visible set (an empty set hiding everything included) and
    /// rebuilds.
    async fn set_visible_calendars(&self, ids: &[CalendarRef]) {
        self.calendar_state
            .lock()
            .expect("calendar mutex poisoned")
            .visible = Some(
            ids.iter()
                .map(|id| (id.account.clone(), id.key.key().as_str().to_owned()))
                .collect(),
        );
        self.rebuild_calendar().await;
    }

    /// Widens the store's expansion horizon to cover the current window when
    /// it has escaped the watermark, in one coarse chunk per widening.
    async fn ensure_horizon(&self) {
        let window = self.calendar_inputs().window;
        self.widen_horizon_to(window).await;
    }

    /// Widens every account's expansion horizon to cover `span` when it has
    /// escaped the watermark, in one coarse chunk per widening, and reports
    /// whether every store now covers it.
    ///
    /// The watermark is shared across accounts, so one widening expands every
    /// account's store: a span the watermark claims but one store never
    /// expanded to would make that account's later window inside it skip its
    /// own expansion and render confidently empty. The watermark rolls back
    /// to whatever stood before (nothing included) when any account's
    /// expansion refuses (a concurrent sync holds the scope), which is the
    /// exact failure the watermark exists to prevent: re-syncing cannot
    /// repair it, for a delta with no changes derives no occurrences. The
    /// stores an expansion already covered stay covered; an occurrence read
    /// is windowed, so a store wider than the watermark costs nothing.
    async fn widen_horizon_to(&self, span: Horizon) -> bool {
        let (want, previous) = {
            let mut state = self.calendar_state.lock().expect("calendar mutex poisoned");
            let end = span
                .end()
                .checked_add(core::time::Duration::from_secs(HORIZON_CHUNK_DAYS * 86_400))
                .unwrap_or_else(|| span.end());
            let widened = match state.horizon {
                Some((low, high)) if low <= span.start() && end <= high => return true,
                Some((low, high)) => (low.min(span.start()), high.max(end)),
                None => (span.start(), end),
            };
            (widened, state.horizon.replace(widened))
        };
        let horizon = Horizon::new(want.0, want.1).expect("a widened horizon is never empty");
        let zone = self.calendar_inputs().zone;
        let mut widened = true;
        for handle in self.account_handles().await {
            if let Err(err) = self.engine.expand_horizon(&handle.id, horizon, &zone).await {
                widened = false;
                log::warn!("calendar: the horizon could not be widened for one account: {err}");
            }
        }
        if !widened {
            self.calendar_state
                .lock()
                .expect("calendar mutex poisoned")
                .horizon = previous;
        }
        widened
    }

    /// The horizon a refresh's sync covers: the full watermark when one stands
    /// (so a refresh never narrows what the app has expanded), the default
    /// window around now otherwise, matching the window a cold rebuild reads.
    fn sync_horizon(&self) -> Horizon {
        let state = self.calendar_state.lock().expect("calendar mutex poisoned");
        match state.horizon {
            Some((low, high)) => Horizon::new(low, high).unwrap_or_else(|_| {
                Window {
                    start: low,
                    days: 1,
                }
                .horizon()
            }),
            None => default_window(now_utc().unwrap_or_else(|| {
                UtcDateTime::new(2026, 1, 1, 0, 0, 0).expect("a representable instant")
            }))
            .horizon(),
        }
    }
}

/// The wall clock as a UTC instant, the stamp every write and the now line
/// read. Engine time types deliberately cannot read a clock, so the stamp is
/// the caller's.
pub(crate) fn now_utc() -> Option<UtcDateTime> {
    let now = time::OffsetDateTime::now_utc();
    UtcDateTime::new(
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
    .ok()
}

#[cfg(test)]
#[path = "../calendar_dispatch_tests.rs"]
mod dispatch_tests;

#[cfg(test)]
#[path = "../calendar_select_tests.rs"]
mod select_tests;
