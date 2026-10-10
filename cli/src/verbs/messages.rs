// SPDX-License-Identifier: GPL-3.0-only
//! `messages list` and `messages read`: the windowed list read the app's
//! message list is built from, and the whole-message read the reading pane
//! serves. Bodies come from the store's cache; the CLI dials no server, so an
//! uncached body is an error naming that contract rather than a fetch.

use engine_api::{
    AccountId, EmailAddress, Engine, MailListRow, Message, Provider, ProviderKey, UtcDateTime,
};
use serde_json::{json, Value};

use super::folders::runtime;
use super::Answer;
use crate::envelope::Code;

/// How much deeper than `--limit` a folder-filtered list reads, mirroring the
/// app's list policy (`astra-app/src/window.rs`): the window bounds only
/// which conversations appear, so one folder's page does not run short when
/// the account's newest mail is filed elsewhere.
const WINDOW_ROW_FACTOR: usize = 4;

/// The floor on that over-reaching window, the same number the app floors
/// its message-load window at.
const MIN_LOAD_WINDOW: usize = 500;

/// Lists messages newest first. Without `--folder` the window read is the
/// answer; with it, the same window read filtered to the folder's membership
/// and truncated to `limit`, exactly how the app slices a folder's page.
pub(crate) async fn list(
    engine: &Engine,
    account: &AccountId,
    folder: Option<&str>,
    limit: usize,
) -> Answer {
    let window = match folder {
        Some(_) => limit.saturating_mul(WINDOW_ROW_FACTOR).max(MIN_LOAD_WINDOW),
        None => limit,
    };
    let rows = engine
        .mail_window(std::slice::from_ref(account), window)
        .await
        .map_err(runtime("the messages could not be read"))?;
    let messages: Vec<Value> = rows
        .into_iter()
        .filter(|row| in_folder(row, folder))
        .take(limit)
        .map(|row| list_row(&row))
        .collect();
    let mut data = json!({ "account": account.as_str(), "messages": messages });
    if let Some(folder) = folder {
        data["folder"] = json!(folder);
    }
    Ok(data)
}

/// Whether a window row is filed in the asked-for folder.
fn in_folder(row: &MailListRow, folder: Option<&str>) -> bool {
    match folder {
        None => true,
        Some(folder) => row
            .mailboxes
            .iter()
            .any(|mailbox| mailbox.as_str() == folder),
    }
}

/// One list row: the envelope facts an agent needs to pick a message to read.
fn list_row(row: &MailListRow) -> Value {
    json!({
        "key": row.mail.key.as_str(),
        "from": row.mail.from_addr,
        "from_name": row.mail.from_name,
        "subject": row.mail.subject,
        "date": row.mail.date_utc.map(date_word),
        "unread": !row.mail.flags.seen() && !row.mail.flags.draft(),
        "draft": row.mail.flags.draft(),
        "has_attachment": row.mail.has_attachment,
        "preview": row.mail.preview,
    })
}

/// Reads whole messages by provider key. Keys not found are absent from the
/// answer (the engine's contract for moved or tombstoned messages); the body
/// is the store's cached extraction, so a message never read in the app fails
/// the whole verb with the offline contract's wording.
pub(crate) async fn read<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    keys: &[String],
) -> Answer {
    let wanted: Vec<ProviderKey> = keys
        .iter()
        .map(|key| ProviderKey::new(key.clone()))
        .collect::<Result<_, _>>()
        .map_err(|err| (Code::Usage, format!("a --key is malformed: {err}")))?;
    let messages = engine
        .messages_by_keys(account, &wanted)
        .await
        .map_err(runtime("the messages could not be read"))?;
    let mut read: Vec<Value> = Vec::with_capacity(messages.len());
    for message in &messages {
        let body = engine
            .message_body(provider, account, message)
            .await
            .map_err(runtime("a body could not be read"))?;
        read.push(read_row(message, body.plain(), body.html().is_some()));
    }
    Ok(json!({
        "account": account.as_str(),
        "asked": wanted.len(),
        "messages": read,
    }))
}

/// One whole message: the envelope plus the plain-text body. The unsanitized
/// HTML alternative is reported present rather than emitted: agents read the
/// plain form, and rendering the HTML is a host concern.
fn read_row(message: &Message, plain: Option<&str>, html_present: bool) -> Value {
    json!({
        "key": message.id.key().as_str(),
        "message_id": message
            .envelope
            .message_id
            .first()
            .map(|id| id.as_str().to_owned()),
        "from": message.envelope.from.iter().map(address_word).collect::<Vec<_>>(),
        "to": message.envelope.to.iter().map(address_word).collect::<Vec<_>>(),
        "cc": message.envelope.cc.iter().map(address_word).collect::<Vec<_>>(),
        "subject": message.envelope.subject,
        "date": message.received_at.map(date_word),
        "unread": message.is_unread(),
        "draft": message.is_draft(),
        "body": {
            "plain": plain,
            "html_present": html_present,
        },
    })
}

/// An address as one string, `Name <address>` when a name rode the header.
fn address_word(address: &EmailAddress) -> String {
    match &address.name {
        Some(name) => format!("{name} <{}>", address.email),
        None => address.email.clone(),
    }
}

/// A date's wire word, the engine's own canonical UTC spelling.
fn date_word(date: UtcDateTime) -> String {
    date.to_string()
}
