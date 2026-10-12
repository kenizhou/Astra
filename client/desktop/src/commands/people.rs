// SPDX-License-Identifier: GPL-3.0-only
//! The people half of the command surface: the people snapshot pull, the
//! card-seed read the editor opens on, and the recipient-suggestions pull
//! the composer's autosuggest asks with. A child of [`super`], whose
//! command registration this module continues, split the same way the
//! calendar half was to keep each file under the 500-line limit. The
//! snapshot pull mirrors the mailbox-list pair; the seed mirrors the resume
//! read (one blocking engine read on the runtime's driver thread); the
//! suggestions pull needs no leg and no provider, so it rides the same
//! driver-thread shape as the seed for one round trip per keystroke batch.

use astra_app::{AvatarView, CardRef, ContactDraftInput};
use astra_viewmodel::PeopleSnapshot;
use engine_api::{AccountId, ContactId, PersonId, RecipientSuggestion};
use tauri::State;

use crate::AppState;

/// How many suggestions one completion ask requests: the engine's own
/// maximum, spent whole, the same discipline the people page reads under.
pub(crate) const SUGGESTION_LIMIT: usize = 200;

/// The published people snapshot: the unified rows, the open person's
/// detail, the write states, and the create targets.
pub(crate) fn people_of(state: &AppState) -> PeopleSnapshot {
    state.app.people()
}

/// Reads the stored card `card` names and answers the editor's seed: the
/// card's own values in the editor's input subset, or `None` when nobody
/// stores it (nothing to open a form on).
pub(crate) async fn people_card_of(state: &AppState, card: CardRef) -> Option<ContactDraftInput> {
    state.app.card_seed(&card).await
}

/// The ranked suggestions for one completion token: the engine's order,
/// capped at the pull's own limit.
pub(crate) async fn people_suggestions_of(
    state: &AppState,
    token: &str,
) -> Vec<RecipientSuggestion> {
    state
        .app
        .recipient_suggestions(token, SUGGESTION_LIMIT)
        .await
}

/// The avatar one address resolves to: the core's one monogram derivation
/// and the cached photo file's path, so no surface ever derives its own.
pub(crate) async fn people_avatar_of(state: &AppState, email: &str) -> AvatarView {
    state.app.avatar_by_email(email).await
}

/// Pulls the people snapshot.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn people(state: State<'_, AppState>) -> PeopleSnapshot {
    people_of(&state)
}

/// Reads the stored card the editor's reference names and answers its seed,
/// or null when nobody stores it. The ids arrive typed, so the wire's own
/// refusals (a blank account, a zero person, an empty card) happen at the
/// boundary; the reference is then built through its single construction
/// point. The read runs on the runtime's driver thread, like the resume
/// read, so the command waits for it and the editor learns what to open
/// with.
// Tauri's command macro hands `State` by value; the wrapper reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn people_card(
    state: State<'_, AppState>,
    person: PersonId,
    account: AccountId,
    card: ContactId,
) -> Option<ContactDraftInput> {
    // The parts arrived typed, so this cannot refuse in practice; `?` keeps
    // the impossible case an honest empty answer rather than a panic.
    let reference = CardRef::from_parts(person.get(), account.as_str(), card.as_str().to_owned())?;
    state.runtime.block_on(|| people_card_of(&state, reference))
}

/// Pulls the ranked suggestions for the completion token: the composer's
/// autosuggest read, one round trip per ask, the token verbatim (splitting
/// the field is the webview's business, not the host's).
// Tauri's command macro hands `State` by value; the wrapper reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn people_suggestions(
    state: State<'_, AppState>,
    token: String,
) -> Vec<RecipientSuggestion> {
    state
        .runtime
        .block_on(|| people_suggestions_of(&state, &token))
}

/// Resolves one address to its avatar: the monogram every surface shares
/// with the snapshot rows and the cached photo file's path when one stands.
/// The read is store work only, so it rides the driver thread like the seed
/// read beside it.
// Tauri's command macro hands `State` by value; the wrapper reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn people_avatar(state: State<'_, AppState>, email: String) -> AvatarView {
    state.runtime.block_on(|| people_avatar_of(&state, &email))
}

#[cfg(test)]
#[path = "people_tests.rs"]
mod tests;
