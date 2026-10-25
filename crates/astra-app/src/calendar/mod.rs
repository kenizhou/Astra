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
mod detail;
mod drag;
mod edit;
mod horizon;
mod imip;
mod invite;
mod prompt;
mod rule_words;
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

// The visible window and the machinery that widens a store's expansion to
// cover it, beside the preference reads that shape both.
pub(crate) use horizon::{default_window, Window};

/// The calendar view state: what a rebuild projects and the intents move. Held
/// in one place behind one mutex because the rebuild reads all of it and every
/// intent writes exactly one field of it.
#[derive(Default)]
pub(crate) struct CalendarState {
    /// The visible window, or `None` until the first `SetRange`: then the
    /// default window around the build instant, or around the persisted
    /// anchor when the boot seed restored one.
    window: Option<Window>,
    /// Which calendars' events are drawn, or `None` until the first
    /// `SetVisibleCalendars`: then every calendar's. `Some` of an empty set is
    /// the user hiding everything, a state the rebuild must be able to hold.
    visible: Option<HashSet<(AccountId, String)>>,
    /// The view kind the pane renders, seeded from the shape preference at
    /// boot and moved by a `SetRange` that names one.
    view: astra_viewmodel::CalendarViewWire,
    /// The grids' navigation anchor (`SetDate`), for the surfaces that ask
    /// where the view is centred. Seeded from the shape preference at boot:
    /// an anchor from a past launch stands exactly, and `None` is today by
    /// behaviour.
    anchor: Option<engine_api::CalendarDate>,
    /// The horizon the app has widened to, per account shared: what
    /// `expand_horizon` has covered, so a window inside it costs nothing and a
    /// window outside it widens in one coarse step.
    horizon: Option<(UtcDateTime, UtcDateTime)>,
    /// Each account's write state, for the snapshot's write rows. Absent
    /// means no write is in flight and none is unconfirmed.
    writes: HashMap<AccountId, CalendarWriteState>,
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
    /// The state every app starts with: no window, no visible set, no
    /// anchor, no watermark, and no writes. The display zone and the cold
    /// window's length are preferences, read at rebuild time.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The state an app whose preferences name a calendar shape starts with:
    /// the view kind and the anchor the person left, restored exactly, so
    /// the first rebuild paints the diary they were looking at. A shape the
    /// file does not hold (absent, or a file the loader cannot parse) never
    /// reaches here: the defaults are `new`'s, whose anchor is today by
    /// behaviour.
    pub(crate) fn seeded(
        view: astra_viewmodel::CalendarViewWire,
        anchor: Option<engine_api::CalendarDate>,
    ) -> Self {
        Self {
            view,
            anchor,
            ..Self::default()
        }
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
    /// The standing view shape, as the snapshot publishes it for a surface
    /// to restore at boot.
    pub shape: astra_viewmodel::CalendarShape,
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
    /// falls back to when no `SetRange` has landed: the horizon preference's
    /// days around the build instant, a week of it behind now, or around the
    /// restored anchor's own midnight when the boot seed holds one.
    fn calendar_inputs(&self) -> CalendarInputs {
        let zone = self.display_zone();
        let horizon_days = self.horizon_days();
        let state = self.calendar_state.lock().expect("calendar mutex poisoned");
        let now = now_utc().unwrap_or_else(|| {
            UtcDateTime::new(2026, 1, 1, 0, 0, 0).expect("a representable instant")
        });
        let window = state
            .window
            .unwrap_or_else(|| default_window(now, horizon_days, state.anchor, &zone));
        CalendarInputs {
            window: window.horizon(),
            zone,
            shape: astra_viewmodel::CalendarShape {
                view: state.view,
                anchor: state.anchor,
            },
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
            CalendarIntent::SetRange { start, days, view } => {
                self.set_calendar_window(start, days, view).await;
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
                self.pane
                    .lock()
                    .expect("preferences mutex poisoned")
                    .set_calendar_anchor(date);
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
    /// escaped the watermark, records the view kind half of the shape
    /// preference when the intent names one, and rebuilds.
    async fn set_calendar_window(
        &self,
        start: UtcDateTime,
        days: u16,
        view: Option<astra_viewmodel::CalendarViewWire>,
    ) {
        self.calendar_state
            .lock()
            .expect("calendar mutex poisoned")
            .window = Some(Window { start, days });
        if let Some(view) = view {
            self.calendar_state
                .lock()
                .expect("calendar mutex poisoned")
                .view = view;
            self.pane
                .lock()
                .expect("preferences mutex poisoned")
                .set_calendar_view(view);
        }
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

#[cfg(test)]
#[path = "../calendar_shape_tests.rs"]
mod shape_tests;
