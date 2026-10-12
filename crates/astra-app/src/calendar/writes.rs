// SPDX-License-Identifier: GPL-3.0-only
//! The write tail: every calendar write intent's shared shape. Each write is
//! gated on the target calendar's `may_write` (read from the store's own copy
//! of the access rights, because a write into an unknown calendar is not a
//! write this app can stand behind), every occurrence-scoped one re-mints its
//! token against the store before it writes (a token is an identifier this
//! core minted, never a time to trust), each write is marked in flight in the
//! view state before the engine call, and settled from the reconcile the
//! engine's facade already ran: `Applied` clears the row, anything else means
//! the write **landed** (only the local confirmation is missing) and marks
//! `FailedWithRetry`, whose one recovery is the re-read a refresh performs. A
//! write the engine refuses outright marks the same row, because a change
//! that silently went nowhere is indistinguishable on screen from one that
//! was never asked for.
//!
//! Every write carries a fresh idempotency key (the engine's outbox is
//! idempotent by it) and every create a fresh cross-system `UID`, both minted
//! here: the host cannot mint a valid `Uid` and should not be asked to.

use astra_viewmodel::CalendarWriteState;
use engine_api::{AccountId, CalendarDateTime, Provider, Reconciled};

use crate::{
    calendar::{
        calendar_provider_of,
        drag::apply_drag,
        edit::{self, NewEvent},
        now_utc,
        snapshot::series_edit_warns,
    },
    reference::CalendarRef,
    App, EventRef,
};

impl<P: Provider> App<P> {
    /// The create arm: gated on the target calendar's write right, under a
    /// core-minted `UID` and stamp. The parameters are the create intent's own
    /// fields, one for one.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn create_calendar_event(
        &self,
        calendar: &CalendarRef,
        summary: &str,
        start: &CalendarDateTime,
        end: &CalendarDateTime,
        description: Option<&str>,
        location: Option<&str>,
        recurrence: Option<&crate::protocol::RepeatRule>,
    ) {
        let Some(handle) = self.account_handle(&calendar.account).await else {
            log::warn!("calendar: the calendar to create in is no longer configured");
            return;
        };
        let Some(provider) = calendar_provider_of(&handle) else {
            log::warn!("calendar: the account has no provider to write through");
            return;
        };
        if !self
            .calendar_may_write(&calendar.account, &calendar.key)
            .await
        {
            log::info!("calendar: the calendar is not writable; the create was refused");
            return;
        }
        let new = NewEvent {
            calendar: calendar.key.clone(),
            summary,
            start,
            end,
            description,
            location,
            recurrence,
        };
        let Some(stamp) = now_utc() else {
            log::warn!("calendar: the wall clock is not representable; the create was refused");
            return;
        };
        let draft = match edit::build_draft(
            &new,
            engine_api::Uid::new(mint_event_uid()).expect("a minted uid"),
            stamp,
            self.calendar_week_start(),
        ) {
            Ok(draft) => draft,
            Err(err) => {
                log::info!("calendar: the create could not be built: {err}");
                return;
            }
        };
        self.mark_calendar_write(&calendar.account, CalendarWriteState::Writing);
        self.rebuild_calendar().await;
        match self
            .engine
            .create_calendar_event(
                provider,
                &calendar.account,
                &generated_idempotency(),
                &draft,
            )
            .await
        {
            Ok(write) => {
                let at = self.account_ordinal(&calendar.account).await;
                log::info!("calendar[a{at}]: an event was created");
                self.settle_calendar_write(&calendar.account, &write.reconciled)
                    .await;
            }
            Err(err) => {
                log::warn!("calendar: the create was refused: {err}");
                self.fail_calendar_write(&calendar.account).await;
            }
        }
    }

    /// The edit arm: the master re-read from the store, the edit rendered in
    /// its own form, the write through the engine's patch verb so properties
    /// this model does not carry survive.
    pub(crate) async fn update_calendar_event(
        &self,
        event: &EventRef,
        edit: &crate::protocol::EventEdit,
    ) {
        let Some((handle, base)) = self.writable_event(event).await else {
            return;
        };
        let Some(provider) = calendar_provider_of(&handle) else {
            log::warn!("calendar: the account has no provider to write through");
            return;
        };
        if let Err(err) = self
            .check_occurrence_token(&event.account, &base, edit.occurrence)
            .await
        {
            log::info!("calendar: the edit was refused: {err}");
            return;
        }
        let Some(stamp) = now_utc() else {
            log::warn!("calendar: the wall clock is not representable; the edit was refused");
            return;
        };
        let (target, patch) =
            match edit::build_patch(&base, edit, stamp, self.calendar_week_start()) {
                Ok(built) => built,
                Err(err) => {
                    log::info!("calendar: the edit could not be built: {err}");
                    return;
                }
            };
        self.patch_calendar(provider, &event.account, &base, target, patch)
            .await;
    }

    /// The move arm: the drag applied to the event's own wall clock, then
    /// through the same patch path an edit takes.
    pub(crate) async fn move_calendar_event(
        &self,
        event: &EventRef,
        drag: &crate::protocol::EventDrag,
    ) {
        let Some((handle, base)) = self.writable_event(event).await else {
            return;
        };
        let Some(provider) = calendar_provider_of(&handle) else {
            log::warn!("calendar: the account has no provider to write through");
            return;
        };
        let drag_edit = match apply_drag(&base, drag) {
            Ok(edit) => edit,
            Err(err) => {
                log::info!("calendar: the drag could not be applied: {err}");
                return;
            }
        };
        if let Err(err) = self
            .check_occurrence_token(&event.account, &base, drag.occurrence)
            .await
        {
            log::info!("calendar: the move was refused: {err}");
            return;
        }
        let Some(stamp) = now_utc() else {
            log::warn!("calendar: the wall clock is not representable; the move was refused");
            return;
        };
        let (target, patch) =
            match edit::build_patch(&base, &drag_edit, stamp, self.calendar_week_start()) {
                Ok(built) => built,
                Err(err) => {
                    log::info!("calendar: the moved edit could not be built: {err}");
                    return;
                }
            };
        self.patch_calendar(provider, &event.account, &base, target, patch)
            .await;
    }

    /// The shared tail of an edit and a move: mark the write in flight, patch,
    /// settle the outcome. A series patch on a series the warning marks is
    /// the one write that can cost the user work they did weeks ago, so it is
    /// named in the log here even though the asking is the surface's (the
    /// row's flag is what a surface asks with; the core cannot ask).
    async fn patch_calendar(
        &self,
        provider: &P,
        account: &AccountId,
        base: &engine_api::Event,
        target: engine_api::PatchTarget,
        patch: engine_api::EventPatch,
    ) {
        if base.is_recurring()
            && matches!(target, engine_api::PatchTarget::Series)
            && series_edit_warns(
                provider.connection_info().capabilities.override_survival(),
                base.recurrence
                    .as_ref()
                    .is_some_and(|recurrence| !recurrence.overrides.is_empty()),
            )
        {
            log::warn!(
                "calendar: the series edit may discard the occurrences changed individually; \
                 the server does not keep them"
            );
        }
        self.mark_calendar_write(account, CalendarWriteState::Writing);
        self.rebuild_calendar().await;
        match self
            .engine
            .patch_calendar_event(
                provider,
                account,
                &generated_idempotency(),
                base,
                target,
                patch,
            )
            .await
        {
            Ok(write) => {
                let at = self.account_ordinal(account).await;
                log::info!("calendar[a{at}]: an event was edited");
                self.settle_calendar_write(account, &write.reconciled).await;
            }
            Err(err) => {
                log::warn!("calendar: the edit was refused: {err}");
                self.fail_calendar_write(account).await;
            }
        }
    }

    /// The delete arm: the whole series, or the one occurrence the token names.
    pub(crate) async fn delete_calendar_event(
        &self,
        event: &EventRef,
        occurrence: Option<engine_api::LocalDateTime>,
    ) {
        let Some((handle, base)) = self.writable_event(event).await else {
            return;
        };
        let Some(provider) = calendar_provider_of(&handle) else {
            log::warn!("calendar: the account has no provider to write through");
            return;
        };
        if let Err(err) = self
            .check_occurrence_token(&event.account, &base, occurrence)
            .await
        {
            log::info!("calendar: the delete was refused: {err}");
            return;
        }
        let Some(stamp) = now_utc() else {
            log::warn!("calendar: the wall clock is not representable; the delete was refused");
            return;
        };
        let deletion = match edit::build_deletion(&base, occurrence, stamp) {
            Ok(deletion) => deletion,
            Err(err) => {
                log::info!("calendar: the delete could not be built: {err}");
                return;
            }
        };
        self.mark_calendar_write(&event.account, CalendarWriteState::Writing);
        self.rebuild_calendar().await;
        match self
            .engine
            .delete_calendar_event(
                provider,
                &event.account,
                &generated_idempotency(),
                Some(&base),
                &deletion,
            )
            .await
        {
            Ok(deleted) => {
                let at = self.account_ordinal(&event.account).await;
                log::info!("calendar[a{at}]: an event was deleted");
                self.settle_calendar_write(&event.account, &deleted.reconciled)
                    .await;
            }
            Err(err) => {
                log::warn!("calendar: the delete was refused: {err}");
                self.fail_calendar_write(&event.account).await;
            }
        }
    }

    /// The master the reference names, re-read from the store, with its
    /// calendar's write right checked: the gate every event write consults.
    /// `None`, with a log line, for an event the store no longer holds or a
    /// calendar it may not write.
    async fn writable_event(
        &self,
        event: &EventRef,
    ) -> Option<(std::sync::Arc<crate::Account<P>>, engine_api::Event)> {
        let handle = self.account_handle(&event.account).await?;
        let base = self
            .engine
            .events_by_keys(&event.account, std::slice::from_ref(&event.key))
            .await
            .ok()?
            .into_iter()
            .next()?;
        let calendar = base.calendars.iter().next()?.clone();
        if !self.calendar_may_write(&event.account, &calendar).await {
            log::info!("calendar: the event's calendar is not writable; the write was refused");
            return None;
        }
        Some((handle, base))
    }

    /// Whether `calendar` of `account` may be written, from the store's own
    /// copy of the access rights: `false` for a calendar the store does not
    /// hold, because a write into an unknown calendar is not a write this app
    /// can stand behind.
    async fn calendar_may_write(
        &self,
        account: &AccountId,
        calendar: &engine_api::CalendarId,
    ) -> bool {
        self.engine
            .calendars(account)
            .await
            .ok()
            .and_then(|calendars| {
                calendars
                    .iter()
                    .find(|listed| listed.id == *calendar)
                    .map(|listed| listed.access.may_write)
            })
            .unwrap_or(false)
    }

    /// Records a write state for `account` (replacing whatever stood there:
    /// the next write resolves the last one's row).
    pub(crate) fn mark_calendar_write(&self, account: &AccountId, state: CalendarWriteState) {
        self.calendar_state
            .lock()
            .expect("calendar mutex poisoned")
            .writes
            .insert(account.clone(), state);
    }

    /// Drops the account's per-account calendar view state: its write rows,
    /// its reply fallbacks, and its rows in the visible set. The teardown a
    /// removed account and a cleared dav leg perform, so a re-added account
    /// inherits no stale state.
    pub(crate) fn clear_account_calendar_state(&self, account: &AccountId) {
        let mut state = self.calendar_state.lock().expect("calendar mutex poisoned");
        state.writes.remove(account);
        state.reply_fallbacks.remove(account);
        if let Some(visible) = &mut state.visible {
            visible.retain(|(held, _)| held != account);
        }
    }

    /// Clears `account`'s write state: the recovery a successful re-read or a
    /// cleanly settled write performs.
    pub(crate) fn clear_calendar_write(&self, account: &AccountId) {
        self.calendar_state
            .lock()
            .expect("calendar mutex poisoned")
            .writes
            .remove(account);
    }

    /// Settles a landed write's outcome: `Applied` clears the account's row,
    /// anything else marks it `FailedWithRetry` (the write landed; only the
    /// local confirmation is missing), and either way the rebuilt snapshot is
    /// what tells the surface.
    pub(crate) async fn settle_calendar_write(&self, account: &AccountId, reconciled: &Reconciled) {
        match reconciled {
            Reconciled::Applied(_) => self.clear_calendar_write(account),
            // Busy and Failed both mean the write landed and only the local
            // copy is stale; a reconcile state this build has not met is read
            // the same conservative way, because the safe recovery for any of
            // them is the re-read a refresh performs.
            _ => {
                self.mark_calendar_write(account, CalendarWriteState::FailedWithRetry);
            }
        }
        self.rebuild_calendar().await;
    }

    /// Marks the account's row for a write the engine refused outright: the
    /// change went nowhere, and a surface that watched the write start must
    /// also see that it did not settle. The row's recovery is the same
    /// re-read a refresh performs, which shows the change is not there and
    /// may be made again.
    async fn fail_calendar_write(&self, account: &AccountId) {
        self.mark_calendar_write(account, CalendarWriteState::FailedWithRetry);
        self.rebuild_calendar().await;
    }
}

/// A unique-per-intent idempotency key for a calendar write, from the shared
/// mint under the family's own prefix.
pub(crate) fn generated_idempotency() -> String {
    crate::mint_idempotency("cal")
}

/// A fresh cross-system `UID` for a created event: globally unique by the
/// clock and the process counter, under the product's own domain.
fn mint_event_uid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("astra-{nanos}-{seq}@astra.local")
}
