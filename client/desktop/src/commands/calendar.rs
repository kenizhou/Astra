// SPDX-License-Identifier: GPL-3.0-only
//! The calendar half of the command surface: the calendar snapshot pull
//! and the meeting brief generation, riding the app core's own drive. A
//! child of [`super`], whose command registration this module continues,
//! split the same way the replies and documents modules were to keep each
//! file under the 500-line limit. The snapshot pull mirrors the
//! mailbox-list pair; the brief generate mirrors the digest generate (one
//! blocking generation on the runtime's driver thread, the turn's
//! correlation minted inside the core and landing in the ledger with it).

use astra_app::{BriefDocumentError, EventRef, MeetingBriefView};
use astra_viewmodel::{CalendarSnapshot, EventDetail};
use engine_api::{AccountId, CalendarDate};
use tauri::State;

use crate::commands::CommandError;
use crate::AppState;

/// The published calendar snapshot: the manager set, the visible window's
/// joined occurrences, the write states, and the now line data.
pub(crate) fn calendar_of(state: &AppState) -> CalendarSnapshot {
    state.app.calendar()
}

/// Reads one event's detail over the reference the webview names: the
/// roll-call (the organiser first, the tail folded into a count) and the
/// full repeating rule with its read-back sentence, or `None` when the
/// account's store holds no event at the key.
pub(crate) async fn calendar_event_detail_of(
    state: &AppState,
    account: &str,
    key: &str,
) -> Option<EventDetail> {
    let reference = EventRef::from_parts(account, key.to_owned())?;
    state.app.calendar_event_detail(&reference).await
}

/// Generates the account's meeting brief over the civil day: the core's
/// own store re-read and one generation, answered as the brief's view
/// (the no-meetings state included, an empty day spending no call).
///
/// # Errors
///
/// Returns [`CommandError::Ai`] when no AI bundle runs, the day cannot be
/// read, or the generation fails; no content rides a failure.
pub(crate) async fn calendar_brief_generate_of(
    state: &AppState,
    account: &AccountId,
    day: CalendarDate,
) -> Result<MeetingBriefView, CommandError> {
    state
        .app
        .generate_meeting_brief(account, day)
        .await
        .map_err(|err: BriefDocumentError| CommandError::Ai(err.to_string()))
}

/// Pulls the calendar snapshot.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn calendar(state: State<'_, AppState>) -> CalendarSnapshot {
    calendar_of(&state)
}

/// Reads one event's detail: the roll-call of who is on the event and the
/// full repeating rule's read-back sentence, or null when nobody holds the
/// event the webview named. The read is one targeted engine lookup on the
/// runtime's driver thread, like the card seed the people editor rides.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn calendar_event_detail(
    state: State<'_, AppState>,
    account: String,
    key: String,
) -> Option<EventDetail> {
    state
        .runtime
        .block_on(|| calendar_event_detail_of(&state, &account, &key))
}

/// Generates the account's meeting brief for the civil day, storing
/// nothing: every ask re-reads the day and answers fresh.
///
/// # Errors
///
/// Returns [`CommandError::Ai`] when the generation cannot run; the
/// panel surfaces the sentence.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn calendar_brief_generate(
    state: State<'_, AppState>,
    account: AccountId,
    day: CalendarDate,
) -> Result<MeetingBriefView, CommandError> {
    state
        .runtime
        .block_on(|| calendar_brief_generate_of(&state, &account, day))
}

#[cfg(test)]
#[path = "calendar_tests.rs"]
mod tests;
