// SPDX-License-Identifier: GPL-3.0-only
//! The chat boundary between the command surface and the relay: the
//! id-derived session every chat's prompts ride, the title a minted chat
//! takes, and the store reads and writes the four chat commands wrap.
//! Prompts with no chat (the tests, the headless paths) ride the fixed
//! main session and record nothing: the boundary is the caller's `chat`
//! argument, never a hidden default thread.

use astra_ai::{AiStore, ChatId, ChatMessageRow, ChatRow, NewChat};

use crate::commands::CommandError;

/// The prefix every chat's session id carries; the ids are UUIDs, so the
/// space never collides with the fixed `agent:main`.
const CHAT_SESSION_PREFIX: &str = "agent:chat:";

/// The cap on a minted chat's title: the thread list's label stays one
/// line.
const TITLE_CAP: usize = 48;

/// The session id one chat's prompts ride. The relay never parses it
/// back: the turn book keeps the chat id itself, so the derivation has
/// exactly one direction.
pub(crate) fn session_for(chat: &ChatId) -> String {
    format!("{CHAT_SESSION_PREFIX}{}", chat.as_str())
}

/// The title a minted chat takes: the prompt's first line, trimmed and
/// capped on a character boundary so the thread list's label stays one
/// line whatever the prompt's shape.
pub(crate) fn title_from_prompt(prompt: &str) -> String {
    let first_line = prompt.lines().next().unwrap_or_default().trim();
    if first_line.chars().count() <= TITLE_CAP {
        return first_line.to_owned();
    }
    let cut = first_line
        .char_indices()
        .nth(TITLE_CAP)
        .map_or(first_line.len(), |(at, _)| at);
    first_line[..cut].to_owned()
}

/// The chat a prompt rides: a caller-named chat kept, an absent one
/// minted with the prompt's first line as its title. Minting here is the
/// product's own shape: the first prompt of a fresh panel starts a
/// thread, and the receipt names it so the panel can keep riding it.
pub(crate) fn resolve_chat(
    store: &AiStore,
    chat_id: Option<&str>,
    prompt: &str,
) -> Result<ChatId, CommandError> {
    let store_read = |e: astra_ai::AiStoreError| CommandError::Agent(e.to_string());
    match chat_id {
        Some(id) => {
            let id = ChatId::new(id.to_owned());
            if store.chat_exists(&id).map_err(store_read)? {
                Ok(id)
            } else {
                Err(CommandError::Agent(format!(
                    "no chat is stored under {id}; it may have been deleted"
                )))
            }
        }
        None => store
            .insert_chat(&NewChat {
                id: ChatId::generate(),
                title: title_from_prompt(prompt),
            })
            .map(|row| row.id)
            .map_err(store_read),
    }
}

/// Every chat, the most recently touched first: the panel's thread list.
pub(crate) fn chat_list_of(store: &AiStore) -> Result<Vec<ChatRow>, CommandError> {
    store
        .list_chats()
        .map_err(|e| CommandError::Agent(e.to_string()))
}

/// One chat's messages, oldest first: the rows the panel paints a loaded
/// thread from. An unknown chat answers empty, the state of a thread with
/// no turns.
pub(crate) fn chat_messages_of(
    store: &AiStore,
    chat_id: &str,
) -> Result<Vec<ChatMessageRow>, CommandError> {
    store
        .chat_messages(&ChatId::new(chat_id.to_owned()))
        .map_err(|e| CommandError::Agent(e.to_string()))
}

/// Rewrites one chat's title, answering whether any row changed.
pub(crate) fn chat_rename_of(
    store: &AiStore,
    chat_id: &str,
    title: &str,
) -> Result<bool, CommandError> {
    store
        .rename_chat(&ChatId::new(chat_id.to_owned()), title)
        .map_err(|e| CommandError::Agent(e.to_string()))
}

/// Deletes one chat together with its messages, answering whether any row
/// went.
pub(crate) fn chat_delete_of(store: &AiStore, chat_id: &str) -> Result<bool, CommandError> {
    store
        .delete_chat(&ChatId::new(chat_id.to_owned()))
        .map_err(|e| CommandError::Agent(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The title caps on a character boundary, never a byte half of one.
    #[test]
    fn the_title_is_the_first_line_capped_whole() {
        assert_eq!(title_from_prompt("Short question"), "Short question");
        let long_line = "A first line far longer than any thread title should ever carry";
        assert_eq!(
            title_from_prompt(&format!("{long_line}\nsecond line")),
            &long_line[..TITLE_CAP.min(long_line.len())],
            "the cap trims the first line"
        );
        assert_eq!(title_from_prompt("  trimmed  "), "trimmed");
        // A multi-byte character straddling the cap is kept or dropped
        // whole, never sliced into half a character.
        let long = "ä".repeat(60);
        assert_eq!(
            title_from_prompt(&long).chars().count(),
            TITLE_CAP,
            "the cap lands on a character boundary"
        );
        assert_eq!(title_from_prompt("\nleading newline"), "");
    }

    /// The session derivation names the chat, one direction only.
    #[test]
    fn the_session_derivation_names_the_chat() {
        let id = ChatId::new("b7c6a5d4".to_owned());
        assert_eq!(session_for(&id), "agent:chat:b7c6a5d4");
        assert_eq!(
            session_for(&ChatId::new("main".to_owned())),
            "agent:chat:main",
            "the prefix keeps a chat's session from ever being the fixed main session's equal"
        );
    }
}
