// SPDX-License-Identifier: GPL-3.0-only
//! `messages list`, `messages read`, and `messages search`: the windowed
//! list read the app's message list is built from, the whole-message read
//! the reading pane serves, and the paged search over the engine's own
//! search surface. Bodies come from the store's cache; the CLI dials no
//! server, so an uncached body is an error naming that contract rather
//! than a fetch.

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

/// How deep one account's ranked search fetch reaches, the app layer's own
/// fetch bound: a candidate set to re-order by date, not the answer's
/// horizon.
const SEARCH_FETCH_LIMIT: usize = 500;

/// The page a search answers when `--limit` names none, the MCP search
/// schema's own default.
pub(crate) const DEFAULT_SEARCH_LIMIT: usize = 20;

/// The largest page a search answers, the MCP search schema's own bound.
pub(crate) const MAX_SEARCH_LIMIT: usize = 50;

/// Searches mail and answers one page of hits, newest first, with no
/// bodies. The scoping is the MCP `search_messages` contract: no named
/// account covers every account in `covered` and every folder except each
/// account's own Trash, a named account (one id in `covered`, `named`)
/// covers its whole mailbox, and a folder narrows within the named account
/// only. Paging is offset plus limit with a `has_more` answer, the
/// reference chat's own pagination discipline.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn search(
    engine: &Engine,
    covered: &[AccountId],
    named: bool,
    folder: Option<&str>,
    query: &str,
    offset: usize,
    limit: usize,
) -> Answer {
    if query.trim().is_empty() {
        return Ok(empty_page(offset));
    }
    let mut hits: Vec<MailListRow> = Vec::new();
    for account in covered {
        let tree = engine
            .mailboxes(account)
            .await
            .map_err(runtime("the folders could not be read"))?;
        let filter = HitFilter::resolve(named, folder, &tree);
        let ranked = engine
            .search_mail(account, query, SEARCH_FETCH_LIMIT)
            .await
            .map_err(|err| (Code::Usage, format!("the --query is malformed: {err}")))?
            .hits;
        let keys: Vec<_> = ranked.iter().map(|hit| hit.key.clone()).collect();
        let resolved = engine
            .mail_by_keys(account, &keys)
            .await
            .map_err(runtime("the messages could not be read"))?;
        hits.extend(resolved.into_iter().filter(|row| filter.keeps(row)));
    }
    hits.sort_by(|a, b| newest_first(a, b).then_with(|| tiebreak(a, b)));
    let total = hits.len();
    let rows: Vec<Value> = hits
        .iter()
        .skip(offset)
        .take(limit)
        .map(search_row)
        .collect();
    let has_more = offset + rows.len() < total;
    Ok(json!({
        "rows": rows,
        "total": total,
        "offset": offset,
        "has_more": has_more,
        // The fetch was the cut when the engine answered a full candidate
        // set: older matches may exist beyond the horizon.
        "windowed": total >= SEARCH_FETCH_LIMIT,
    }))
}

/// One search hit as the MCP query row: identity, subject, sender, date,
/// and flags. No body crosses here.
fn search_row(row: &MailListRow) -> Value {
    json!({
        "account": row.account.as_str(),
        "key": row.mail.key.as_str(),
        "subject": row.mail.subject.clone().unwrap_or_default(),
        "sender": sender_line(row),
        "date": row.mail.date_utc.map(whole_seconds),
        "unread": row.mail.flags.is_unread(),
        "flagged": row.mail.flags.flagged(),
        "has_attachment": row.mail.has_attachment,
    })
}

/// The sender as `Name <address>`, the bare address, or empty when there is
/// no sender at all, the query layer's own spelling.
fn sender_line(row: &MailListRow) -> String {
    let name = row
        .mail
        .from_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let address = row
        .mail
        .from_addr
        .as_deref()
        .filter(|addr| !addr.is_empty());
    match (name, address) {
        (Some(name), Some(address)) if name != address => format!("{name} <{address}>"),
        (_, Some(address)) => address.to_owned(),
        (Some(name), None) => name.to_owned(),
        (None, None) => String::new(),
    }
}

/// A UTC instant as a whole-second `...Z` word, the list projections' own
/// spelling (the engine's canonical form can carry sub-second precision).
fn whole_seconds(instant: UtcDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        instant.year(),
        instant.month(),
        instant.day(),
        instant.hour(),
        instant.minute(),
        instant.second(),
    )
}

/// Newest instant first, undated last.
fn newest_first(a: &MailListRow, b: &MailListRow) -> std::cmp::Ordering {
    std::cmp::Reverse(a.mail.date_utc).cmp(&std::cmp::Reverse(b.mail.date_utc))
}

/// The tiebreak that makes the ordering total: the identity a surface
/// reconciles a row by.
fn tiebreak(a: &MailListRow, b: &MailListRow) -> std::cmp::Ordering {
    (a.account.as_str(), a.mail.key.as_str()).cmp(&(b.account.as_str(), b.mail.key.as_str()))
}

/// The empty page an unusable query answers: no rows, no cut.
fn empty_page(offset: usize) -> Value {
    json!({
        "rows": [],
        "total": 0,
        "offset": offset,
        "has_more": false,
        "windowed": false,
    })
}

/// Which of one account's matched rows the search scope keeps, resolved
/// against that account's own tree.
enum HitFilter {
    /// Everything except the account's Trash; `None` when it has no trash
    /// folder, in which case nothing is excluded.
    ExceptTrash(Option<String>),
    /// The account's whole mailbox, Trash included.
    Everything,
    /// Only the messages filed in this folder key.
    Only(String),
}

impl HitFilter {
    /// The filter for the query's narrowings: a named account searches its
    /// whole mailbox, an unnamed scope searches everything but each
    /// account's Trash, and a folder narrows within a named account.
    fn resolve(named: bool, folder: Option<&str>, tree: &[engine_api::Mailbox]) -> Self {
        match (named, folder) {
            (false, _) => Self::ExceptTrash(trash_key(tree)),
            (true, None) => Self::Everything,
            (true, Some(key)) => Self::Only(key.to_owned()),
        }
    }

    /// Whether this filter keeps `row`.
    fn keeps(&self, row: &MailListRow) -> bool {
        let filed_in = |key: &str| row.mailboxes.iter().any(|mb| mb.as_str() == key);
        match self {
            Self::ExceptTrash(trash) => trash.as_deref().is_none_or(|key| !filed_in(key)),
            Self::Everything => true,
            Self::Only(key) => filed_in(key),
        }
    }
}

/// The provider key of the folder `tree` tags with the Trash role.
fn trash_key(tree: &[engine_api::Mailbox]) -> Option<String> {
    tree.iter()
        .find(|mailbox| mailbox.role.as_ref() == Some(&engine_api::MailboxRole::Trash))
        .map(|mailbox| mailbox.id.key().as_str().to_owned())
}
