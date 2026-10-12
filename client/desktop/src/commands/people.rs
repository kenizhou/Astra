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

/// Reads one cached contact photo as a data URI the CSP already admits.
/// Serves only paths inside the engine's contact-photo area (canonicalised,
/// prefix-checked against the boot's own photo root): the webview never
/// gains a protocol wide enough to read host files itself, and a traversal
/// attempt answers null rather than bytes.
// Tauri's command macro hands `State` by value; the wrapper reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn people_photo(state: State<'_, AppState>, path: String) -> Option<String> {
    photo_data_uri(&state.photo_root, std::path::Path::new(&path))
}

/// The data URI for `path` when it names a photo inside `root`: the type
/// sniffed from the bytes' own magic (the blob files carry no extension),
/// base64-coded for the `img-src data:` the CSP admits.
pub(crate) fn photo_data_uri(root: &std::path::Path, path: &std::path::Path) -> Option<String> {
    let canonical = path.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    if !canonical.starts_with(&root) {
        return None;
    }
    let bytes = std::fs::read(canonical).ok()?;
    let media = sniff_image_type(&bytes)?;
    Some(format!("data:{media};base64,{}", base64_standard(&bytes)))
}

/// Standard base64 (RFC 4648) with padding: the one encoding a data URI
/// reads. Hand-rolled and vector-pinned rather than a dependency for a
/// single avatar path, the same trade the live calendar helper makes.
fn base64_standard(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut buffer = [0u8; 3];
        buffer[..chunk.len()].copy_from_slice(chunk);
        let word = u32::from(buffer[0]) << 16 | u32::from(buffer[1]) << 8 | u32::from(buffer[2]);
        out.push(char::from(ALPHABET[(word >> 18) as usize & 0x3f]));
        out.push(char::from(ALPHABET[(word >> 12) as usize & 0x3f]));
        out.push(if chunk.len() > 1 {
            char::from(ALPHABET[(word >> 6) as usize & 0x3f])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(ALPHABET[word as usize & 0x3f])
        } else {
            '='
        });
    }
    out
}

/// The image type magic bytes name: the formats a contact photo arrives in.
fn sniff_image_type(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "people_tests.rs"]
mod tests;
