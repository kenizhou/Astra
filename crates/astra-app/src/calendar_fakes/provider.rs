// SPDX-License-Identifier: GPL-3.0-only
//! The fake's wire legs: the provider trait answers (mail and calendar
//! syncs, the source read, the submit) and the calendar write verbs, each
//! recording what crossed and applying the series-level ones to the served
//! diary so the reconcile a facade write runs presents the change.

use async_trait::async_trait;
use engine_api::{
    apply_reply, AccountId, Calendar, CalendarWrites, Capabilities, Draft, Duration, Event,
    EventDeletion, EventDraft, EventEdit, EventId, EventRsvp, EventWrite, EventWriteReceipt,
    Mailbox, MailboxEdit, MailboxEditReceipt, MailboxWrites, Message, Provider, ProviderKey,
    Recurrence, RsvpControls, SubmissionReceipt, SyncWindow, Uid, WriteGuard,
};
use engine_core::{
    membership::Memberships,
    raw::RawMime,
    sync::{SyncState, SyncUpdate},
    version::{ETag, RevisionTokens},
};
use engine_provider::{
    ConnectionInfo, EmailChunk, EmailStream, HandOver, ProviderError, ProviderResult, ScopeSync,
    TextEdit,
};

use super::CalendarFake;

#[async_trait]
impl Provider for CalendarFake {
    fn connection_info(&self) -> ConnectionInfo {
        let mut caps = Capabilities::none()
            .with_mail()
            .with_message_source()
            .with_calendars()
            .with_calendar_writes(WriteGuard::Enforced, engine_api::OverrideSurvival::kept())
            .with_calendar_rsvp(RsvpControls {
                comment: self.rsvp_comment,
                suppress_notification: true,
                guard: WriteGuard::Enforced,
            })
            .with_submission()
            .with_scheduling_submission();
        if self.schedules {
            caps = caps.with_calendar_scheduling();
        }
        ConnectionInfo::new(caps)
    }

    async fn sync_mailboxes(
        &self,
        _account: &AccountId,
        _cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Mailbox>> {
        let present = self
            .mailboxes
            .iter()
            .map(|mailbox| mailbox.id.key().clone())
            .collect();
        Ok(ScopeSync::new(
            SyncUpdate::snapshot(self.mailboxes.clone(), present),
            SyncState::new("mbox-1"),
        ))
    }

    fn stream_email<'a>(
        &'a self,
        _account: &'a AccountId,
        _cursor: Option<&'a SyncState>,
        _window: SyncWindow,
        _fetch_batch: usize,
        _chunk_size: usize,
    ) -> EmailStream<'a> {
        let present = self
            .messages
            .iter()
            .map(|message| message.id.key().clone())
            .collect();
        Box::pin(futures::stream::iter(vec![Ok(EmailChunk::reconcile_last(
            self.messages.clone(),
            present,
            Some(self.messages.len()),
            SyncState::new("email-1"),
        ))]))
    }

    async fn fetch_message_source(
        &self,
        _account: &AccountId,
        message: &Message,
    ) -> ProviderResult<RawMime> {
        self.sources
            .get(message.id.key().as_str())
            .cloned()
            .map(RawMime::new)
            .ok_or_else(|| ProviderError::invalid_state("the fake holds no source for that key"))
    }

    async fn sync_calendars(
        &self,
        _account: &AccountId,
        _cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Calendar>> {
        let present = self
            .calendars
            .iter()
            .map(|calendar| calendar.id.key().clone())
            .collect();
        Ok(ScopeSync::new(
            SyncUpdate::snapshot(self.calendars.clone(), present),
            SyncState::new("cal-1"),
        ))
    }

    async fn sync_events(
        &self,
        _account: &AccountId,
        _cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Event>> {
        {
            let mut left = self.fail_syncs.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                return Err(ProviderError::retryable("the server is unreachable"));
            }
        }
        let events = self.events.lock().unwrap().clone();
        let present = events.iter().map(|event| event.id.key().clone()).collect();
        Ok(ScopeSync::new(
            SyncUpdate::snapshot(events, present),
            SyncState::new("evt-1"),
        ))
    }

    async fn submit_email(
        &self,
        _account: &AccountId,
        draft: &Draft,
        hand_over: &HandOver<'_>,
    ) -> ProviderResult<SubmissionReceipt> {
        self.submits.lock().unwrap().push(draft.clone());
        let handed_over = hand_over.commit().await?;
        Ok(SubmissionReceipt::filed(
            ProviderKey::new("sent-1").unwrap(),
            draft.message_id.clone(),
            &handed_over,
        ))
    }
}

#[async_trait]
impl MailboxWrites for CalendarFake {
    async fn edit_mailbox(
        &self,
        _account: &AccountId,
        _edit: &MailboxEdit,
    ) -> ProviderResult<MailboxEditReceipt> {
        Err(ProviderError::invalid_state("the fake holds no tree"))
    }
}

#[async_trait]
impl CalendarWrites for CalendarFake {
    async fn create_event(
        &self,
        _account: &AccountId,
        draft: &EventDraft,
    ) -> ProviderResult<EventWriteReceipt> {
        self.creates.lock().unwrap().push(draft.clone());
        let mut event = Event::new(
            EventId::try_from(format!("/cal/{}.ics", draft.uid.as_str()).as_str()).unwrap(),
            draft.uid.clone(),
            Memberships::of_one(draft.calendar.clone()),
            draft.start.clone(),
        );
        event.title = draft.summary.clone();
        event.description = draft.description.clone();
        event.duration = span_of(&draft.start, &draft.end)?;
        if let Some(repeat) = &draft.recurrence {
            event.recurrence = Some(Recurrence::from_rule(repeat.rule.clone()));
        }
        self.events.lock().unwrap().push(event.clone());
        Ok(receipt(
            event.id,
            event.uid,
            engine_api::ReplyDelivery::NotReported,
        ))
    }

    async fn patch_event(
        &self,
        _account: &AccountId,
        base: &Event,
        edit: &EventEdit,
    ) -> ProviderResult<EventWriteReceipt> {
        self.patches
            .lock()
            .unwrap()
            .push((base.clone(), edit.clone()));
        // A series patch is applied to the served copy; an occurrence patch is
        // recorded only (the override surgery is the adapters' business).
        if edit.target == engine_api::PatchTarget::Series {
            let mut events = self.events.lock().unwrap();
            if let Some(served) = events.iter_mut().find(|served| served.id == base.id) {
                let patch = &edit.patch;
                if let Some(title) = patch.summary_edit() {
                    served.title = title.to_owned();
                }
                if let Some(edit) = patch.description_edit() {
                    served.description = match edit {
                        TextEdit::Set(text) => Some(text.clone()),
                        TextEdit::Clear => None,
                    };
                }
                if let Some(start) = patch.start_edit() {
                    served.start = start.clone();
                }
                if patch.start_edit().is_some() || patch.end_edit().is_some() {
                    let end = patch
                        .end_edit()
                        .cloned()
                        .unwrap_or_else(|| served.start.clone());
                    served.duration = span_of(&served.start, &end)?;
                }
                if let Some(edit) = patch.recurrence_edit() {
                    served.recurrence = match edit {
                        engine_api::RecurrenceEdit::Set(rule) => {
                            Some(Recurrence::from_rule(rule.rule.clone()))
                        }
                        engine_api::RecurrenceEdit::Clear => None,
                    };
                }
            }
        }
        Ok(receipt(
            edit.event.clone(),
            edit.uid.clone(),
            engine_api::ReplyDelivery::NotReported,
        ))
    }

    async fn put_event(
        &self,
        _account: &AccountId,
        write: &EventWrite,
    ) -> ProviderResult<EventWriteReceipt> {
        self.puts.lock().unwrap().push(write.clone());
        // The stored document joins the diary as the event the test taught the
        // fake for its `UID`, under the address the write named: the honest
        // stand-in for a server parsing the bytes.
        if let Some(mut stored) = self.documents.get(write.uid.as_str()).cloned() {
            stored.id = write.event.clone();
            self.events.lock().unwrap().push(stored);
        }
        Ok(receipt(
            write.event.clone(),
            write.uid.clone(),
            engine_api::ReplyDelivery::NotReported,
        ))
    }

    async fn rsvp_event(
        &self,
        _account: &AccountId,
        base: &Event,
        rsvp: &EventRsvp,
    ) -> ProviderResult<EventWriteReceipt> {
        self.rsvps
            .lock()
            .unwrap()
            .push((base.clone(), rsvp.clone()));
        let mut events = self.events.lock().unwrap();
        if let Some(served) = events.iter_mut().find(|served| served.uid == base.uid) {
            apply_reply(served, &rsvp.attendee, rsvp.response.status());
        }
        Ok(receipt(
            rsvp.event.clone(),
            rsvp.uid.clone(),
            self.reply_delivery.clone(),
        ))
    }

    async fn delete_event(
        &self,
        _account: &AccountId,
        base: Option<&Event>,
        deletion: &EventDeletion,
    ) -> ProviderResult<()> {
        self.deletes
            .lock()
            .unwrap()
            .push((base.cloned(), deletion.clone()));
        if deletion.occurrence_target().is_none() {
            self.events
                .lock()
                .unwrap()
                .retain(|served| served.uid != deletion.uid);
        }
        Ok(())
    }
}

/// A successful receipt under a fixed etag, carrying the RSVP delivery the
/// fake was told to report.
fn receipt(event: EventId, uid: Uid, delivery: engine_api::ReplyDelivery) -> EventWriteReceipt {
    EventWriteReceipt::new(event, uid, RevisionTokens::from_etag(ETag::new("\"v1\"")))
        .with_reply_delivery(delivery)
}

/// The duration between two calendar values, in the engine's own calendar
/// arithmetic: what a real adapter derives from the pair the way its own
/// protocol states it.
fn span_of(
    start: &engine_api::CalendarDateTime,
    end: &engine_api::CalendarDateTime,
) -> ProviderResult<Duration> {
    start
        .duration_until(end)
        .map_err(|err| ProviderError::invalid_state(err.to_string()))
}
