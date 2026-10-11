// SPDX-License-Identifier: GPL-3.0-only
//! The meeting brief's app-facing half: the pull the host commands ride.
//! It re-reads the day's occurrences from the store through the same
//! join the snapshot rebuild rides (occurrence rows plus their masters,
//! never a cached snapshot field), widens the expansion horizon when the
//! day has escaped it, and links each meeting's stored invitation by
//! `UID` over a bounded scan of the account's newest mail, so the brief
//! names the invitation's subject where one exists. An empty day answers
//! the defined no-meetings state and spends no call at all.

use std::collections::{HashMap, HashSet};

use engine_api::{
    resolve_instant_in, CalendarDate, CalendarDateTime, Event, Horizon, ParticipantRole, Provider,
    ProviderKey,
};

use crate::calendar::brief::{
    generate_meeting_brief, BriefDocumentError, BriefMeeting, BriefMeetingRow, MeetingBriefState,
    MeetingBriefView, BRIEF_INVITATION_SCAN_LIMIT,
};
use crate::calendar::snapshot::diary_participation;
use crate::App;

impl<P: Provider> App<P> {
    /// Generates the account's meeting brief over `day`: the day's
    /// occurrences re-read from the store through the same join the
    /// snapshot rebuild rides (never a cached snapshot field), the linked
    /// invitation context where a stored invitation names a meeting, and
    /// ONE generation under a correlation minted for the pull. An empty
    /// day answers the no-meetings state and spends no call.
    ///
    /// # Errors
    /// [`BriefDocumentError::NoBundle`] when the app runs no AI bundle;
    /// the day and generation failures as [`BriefDocumentError`] names
    /// them, no content riding a failure.
    pub async fn generate_meeting_brief(
        &self,
        account: &engine_api::AccountId,
        day: CalendarDate,
    ) -> Result<MeetingBriefView, BriefDocumentError> {
        let pass = self.ai.as_ref().ok_or(BriefDocumentError::NoBundle)?;
        let window = self.day_window(day)?;
        self.ensure_day_horizon(window).await?;
        let rows = self
            .engine
            .occurrences_in(account, window)
            .await
            .map_err(|err| BriefDocumentError::Occurrence(err.to_string()))?;
        let keys: Vec<ProviderKey> = rows
            .iter()
            .map(|row| row.event.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let masters = self
            .engine
            .events_by_keys(account, &keys)
            .await
            .map_err(|err| BriefDocumentError::Occurrence(err.to_string()))?;
        let masters: HashMap<String, Event> = masters
            .into_iter()
            .map(|event| (event.id.key().as_str().to_owned(), event))
            .collect();
        let handle = self.account_handle(account).await;
        let my_address = handle
            .as_ref()
            .map(|handle| handle.identity.email.as_str().to_owned())
            .unwrap_or_default();
        let mut meetings = Vec::new();
        let mut uids = Vec::new();
        for row in rows {
            let Some(master) = masters.get(row.event.as_str()) else {
                continue;
            };
            // A tombstone the store still lists, and a meeting this
            // account declined, brief nowhere: the diary rules the
            // snapshot rebuild applies, applied to the brief's own read.
            if master.is_cancelled()
                || diary_participation(master, &my_address)
                    == astra_viewmodel::Participation::Declined
            {
                continue;
            }
            uids.push(master.uid.as_str().to_owned());
            meetings.push(BriefMeeting {
                title: master.title.clone(),
                start: row.start,
                end: row.end,
                all_day: master.is_all_day(),
                organizer: organizer_of(master),
                attendees: attendees_of(master),
                location: location_of(master),
                invitation_subject: None,
            });
        }
        if meetings.is_empty() {
            log::debug!("calendar: {day} holds no meetings for {account}");
            return Ok(MeetingBriefView {
                account: account.clone(),
                day,
                state: MeetingBriefState::NoMeetings,
                brief: String::new(),
                meetings: Vec::new(),
            });
        }
        // The invitation link is context, never a dependency: the scan is
        // spent only now that a meeting survived the diary's rules, so an
        // empty or all-declined day pays nothing for it.
        let subjects = self.invitation_subjects(account).await;
        for (meeting, uid) in meetings.iter_mut().zip(&uids) {
            meeting.invitation_subject = subjects.get(uid).cloned();
        }
        let view = MeetingBriefView {
            account: account.clone(),
            day,
            state: MeetingBriefState::NoMeetings,
            brief: String::new(),
            meetings: meetings
                .iter()
                .map(|meeting| BriefMeetingRow {
                    title: meeting.title.clone(),
                    start: meeting.start,
                    end: meeting.end,
                    all_day: meeting.all_day,
                    organizer: meeting.organizer.clone(),
                    attendees: meeting.attendees.clone(),
                    location: meeting.location.clone(),
                    invitation_subject: meeting.invitation_subject.clone(),
                })
                .collect(),
        };
        let correlation = astra_ai::CorrelationId::generate();
        let brief = generate_meeting_brief(
            account,
            day,
            &meetings,
            &correlation,
            pass.complete.as_ref(),
        )
        .await?;
        Ok(MeetingBriefView {
            state: MeetingBriefState::Generated,
            brief,
            ..view
        })
    }

    /// The civil day as a half-open UTC window, resolved through the
    /// view's display zone: the day's own midnight to the next day's, so
    /// a daylight-saving change never clips an hour off the day.
    fn day_window(&self, day: CalendarDate) -> Result<Horizon, BriefDocumentError> {
        let zone = self.calendar_inputs().zone;
        let resolve = |date: CalendarDate| {
            resolve_instant_in(&CalendarDateTime::Date(date), &zone)
                .map_err(|err| BriefDocumentError::Occurrence(err.to_string()))
        };
        let start = resolve(day)?;
        let end = resolve(next_date(day).ok_or_else(|| {
            BriefDocumentError::Occurrence(
                "the day after the brief day is unrepresentable".to_owned(),
            )
        })?)?;
        Horizon::new(start, end).map_err(|err| BriefDocumentError::Occurrence(err.to_string()))
    }

    /// Widens every account's expansion horizon to cover the brief day when
    /// it has escaped the watermark, through the same widening discipline
    /// the `SetRange` intent rides: the watermark is shared, so the widening
    /// expands every account's store and rolls the watermark back when any
    /// store refuses. A refused widening is a day the store cannot see: the
    /// pull answers the read failure rather than brief a day nothing
    /// materialized.
    async fn ensure_day_horizon(&self, window: Horizon) -> Result<(), BriefDocumentError> {
        if self.widen_horizon_to(window).await {
            Ok(())
        } else {
            Err(BriefDocumentError::Occurrence(
                "the day's horizon could not be widened".to_owned(),
            ))
        }
    }

    /// The invitation subjects keyed by the meeting `UID` they name, over
    /// a bounded scan of the account's newest mail: each message that
    /// carries a scheduling part contributes its subject under the part's
    /// event uid, the newest message winning a duplicate. A read or a
    /// parse that fails contributes nothing: the link is context, never a
    /// dependency the brief fails without.
    async fn invitation_subjects(
        &self,
        account: &engine_api::AccountId,
    ) -> HashMap<String, String> {
        let mut subjects = HashMap::new();
        let Some(handle) = self.account_handle(account).await else {
            return subjects;
        };
        let Some(provider) = super::calendar_provider_of(&handle) else {
            return subjects;
        };
        let rows = match self
            .engine
            .mail_window(std::slice::from_ref(account), BRIEF_INVITATION_SCAN_LIMIT)
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                log::warn!("calendar: the brief's invitation scan could not read the mail: {err}");
                return subjects;
            }
        };
        for row in rows {
            let Some(subject) = row.mail.subject.clone() else {
                continue;
            };
            let message = crate::reference::MessageRef {
                account: account.clone(),
                key: row.mail.key.clone(),
            };
            let Some(original) = crate::reading::resolve(&self.engine, &message).await else {
                continue;
            };
            match self
                .engine
                .message_scheduling(provider, account, &original)
                .await
            {
                Ok(Some(scheduling)) => {
                    subjects
                        .entry(scheduling.message.event.uid.as_str().to_owned())
                        .or_insert(subject);
                }
                Ok(None) => {}
                Err(err) => {
                    log::debug!("calendar: a brief scan read failed for one message: {err}");
                }
            }
        }
        subjects
    }
}

/// The organiser's line: the first owner-role participant rendered as the
/// digest renders a sender, when the event carries one.
fn organizer_of(event: &Event) -> Option<String> {
    event
        .participants
        .iter()
        .filter(|participant| participant.has_role(&ParticipantRole::Owner))
        .find_map(line_of)
}

/// The attendee lines: every participant the organiser's own line does
/// not cover, rendered the same way.
fn attendees_of(event: &Event) -> Vec<String> {
    event
        .participants
        .iter()
        .filter(|participant| !participant.has_role(&ParticipantRole::Owner))
        .filter_map(line_of)
        .collect()
}

/// One participant as a line: the name and address together when both
/// ride, either alone when only it does, nothing when neither does.
fn line_of(participant: &engine_api::Participant) -> Option<String> {
    match (&participant.name, &participant.email) {
        (Some(name), Some(email)) if !name.trim().is_empty() => Some(format!("{name} <{email}>")),
        (_, Some(email)) => Some(email.clone()),
        (Some(name), None) if !name.trim().is_empty() => Some(name.clone()),
        _ => None,
    }
}

/// The event's location as one line: the first named place, else its
/// description, else the first virtual location's join URI.
fn location_of(event: &Event) -> Option<String> {
    event
        .locations
        .iter()
        .find_map(|location| {
            location
                .name
                .clone()
                .or_else(|| location.description.clone())
        })
        .or_else(|| {
            event
                .virtual_locations
                .first()
                .map(|virtual_location| virtual_location.uri.clone())
        })
}

/// The day after `day`, through the calendar the `time` crate holds: the
/// DST-exact successor, `None` at the calendar's edge.
fn next_date(day: CalendarDate) -> Option<CalendarDate> {
    let date = time::Date::from_calendar_date(
        day.year(),
        time::Month::try_from(day.month()).ok()?,
        day.day(),
    )
    .ok()?;
    let next = date.checked_add(time::Duration::days(1))?;
    CalendarDate::new(next.year(), u8::from(next.month()), next.day()).ok()
}
