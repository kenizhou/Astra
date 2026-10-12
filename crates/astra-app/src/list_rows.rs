// SPDX-License-Identifier: GPL-3.0-only
//! The message-list projection: the windowed engine rows of the selected folder turned
//! into the [`MessageRowView`]s the snapshot carries, and a search's resolved hits
//! projected the same way. Pure, so both surfaces share one definition of what is on
//! screen; the reads that feed it (and the cache they answer from) live in `snapshot`
//! and `search`, the row types in the view-model.
//!
//! Every ordering here is total. Tied rows (same instant, or both undated) break on
//! `(account, key)`, so two projections of the same mailbox produce the same row
//! sequence whatever order the cache or a store reload supplied.
//!
//! Grouping, in the threaded mode, spans the whole windowed read and not only the
//! selected folder: a conversation shows when **any** member is filed in the folder, but
//! carries its out-of-folder members too, so a Sent reply rides in the conversation it
//! answers. A group that collapses to a single message is projected as a message row,
//! not dressed up as a one-message conversation.

use std::{cmp::Reverse, collections::HashMap, sync::Arc};

use astra_viewmodel::{MessageRowView, RowKey, ViewMode};
use engine_api::{MailListRow, MailRow, ThreadId, UtcDateTime};

/// Projects the windowed `rows` of `folder` into at most `limit` snapshot rows, in the
/// `mode` the list is grouped in.
pub(crate) fn project(
    mode: ViewMode,
    rows: &[Arc<MailListRow>],
    folder: &str,
    limit: usize,
) -> Vec<MessageRowView> {
    match mode {
        ViewMode::Flat => flat(rows, folder, limit),
        ViewMode::Threaded => threaded(rows, folder, limit),
    }
}

/// One message a search resolved: a hit the engine matched, or a completed
/// thread member that did not. A hit is what lists a conversation; the other
/// members ride in it, the same in-scope and out-of-scope split the folder
/// view's threaded projection reads.
pub(crate) struct SearchMessage {
    /// The store row.
    pub(crate) row: Arc<MailListRow>,
    /// Whether this member matched the query.
    pub(crate) matched: bool,
}

/// Projects a search's resolved `messages` into at most `limit` snapshot rows,
/// in the `mode` the list is grouped in: a search obeys the list's grouping
/// like any other list. Flat, every hit newest first; threaded, the
/// conversations the hits belong to, each carried whole, labelled and ordered
/// by its newest matching member, so a reply the query said nothing about
/// neither lists a conversation nor lifts it up the results.
pub(crate) fn search_project(
    mode: ViewMode,
    messages: &[SearchMessage],
    limit: usize,
) -> Vec<MessageRowView> {
    match mode {
        ViewMode::Flat => search_flat(messages, limit),
        ViewMode::Threaded => search_threaded(messages, limit),
    }
}

/// The hits, newest first.
fn search_flat(messages: &[SearchMessage], limit: usize) -> Vec<MessageRowView> {
    let mut hits: Vec<&SearchMessage> = messages.iter().filter(|message| message.matched).collect();
    hits.sort_by(|a, b| newest_first(&a.row, &b.row).then_with(|| tiebreak(&a.row, &b.row)));
    hits.into_iter()
        .take(limit)
        .map(|hit| message_row(&hit.row))
        .collect()
}

/// The hit conversations, newest match first.
fn search_threaded(messages: &[SearchMessage], limit: usize) -> Vec<MessageRowView> {
    let mut groups: HashMap<(&str, &str), Vec<&SearchMessage>> = HashMap::new();
    for message in messages {
        let group = (message.row.account.as_str(), thread_key(&message.row));
        groups.entry(group).or_default().push(message);
    }
    // Only conversations a hit touches show, and each is ordered by its latest
    // matching member: the order answers the query, not the mail.
    let mut shown: Vec<((&str, &str), Vec<&SearchMessage>)> = groups
        .into_iter()
        .filter(|(_, members)| members.iter().any(|member| member.matched))
        .collect();
    shown.sort_by(|(key_a, members_a), (key_b, members_b)| {
        let latest_a = latest_match(members_a);
        let latest_b = latest_match(members_b);
        Reverse(latest_a)
            .cmp(&Reverse(latest_b))
            .then_with(|| key_a.cmp(key_b))
    });
    shown
        .into_iter()
        .take(limit)
        .map(|((_, thread), members)| hit_conversation(thread, &members))
        .collect()
}

/// One hit conversation as a snapshot row: its face the newest member that
/// matched (the label that answers the query), its count every member, the
/// non-matching ones included. A group of one is a message row, the same shape
/// it takes in the folder view.
fn hit_conversation(thread: &str, members: &[&SearchMessage]) -> MessageRowView {
    if members.len() == 1 {
        return message_row(&members[0].row);
    }
    let face = members
        .iter()
        .filter(|member| member.matched)
        .min_by(|a, b| newest_first(&a.row, &b.row).then_with(|| tiebreak(&a.row, &b.row)))
        .expect("a shown group holds a matching member");
    // Unreachable for any group that reached here: the key is a resolved thread
    // id or a message's non-empty provider key, and both parse. Guarded rather
    // than unwrapped so a future key shape degrades to a message row instead of
    // panicking a release build.
    let Ok(thread_id) = ThreadId::try_from(thread) else {
        debug_assert!(false, "a conversation key always parses as a thread id");
        return message_row(&face.row);
    };
    let row = &face.row;
    MessageRowView {
        key: RowKey::Thread {
            account: row.account.clone(),
            thread_id,
        },
        account_id: row.account.clone(),
        sender_name: sender_name(&row.mail),
        sender_address: row.mail.from_addr.clone().unwrap_or_default(),
        subject: row.mail.subject.clone().unwrap_or_default(),
        sent_at: row.mail.date_utc.map_or_else(String::new, format_instant),
        unread: members
            .iter()
            .any(|member| member.row.mail.flags.is_unread()),
        flagged: members.iter().any(|member| member.row.mail.flags.flagged()),
        has_attachments: members.iter().any(|member| member.row.mail.has_attachment),
        thread_count: Some(count(members.len())),
    }
}

/// The newest instant among a conversation's matching members; `None` sinks
/// below every dated conversation.
fn latest_match(members: &[&SearchMessage]) -> Option<UtcDateTime> {
    members
        .iter()
        .filter(|member| member.matched)
        .map(|member| member.row.mail.date_utc)
        .max()
        .flatten()
}

/// A flat list of the folder's messages, newest first.
fn flat(rows: &[Arc<MailListRow>], folder: &str, limit: usize) -> Vec<MessageRowView> {
    let mut in_folder: Vec<&Arc<MailListRow>> =
        rows.iter().filter(|row| filed_in(row, folder)).collect();
    in_folder.sort_by(|a, b| newest_first(a, b).then_with(|| tiebreak(a, b)));
    in_folder
        .into_iter()
        .take(limit)
        .map(|row| message_row(row))
        .collect()
}

/// One grouped conversation awaiting projection: its `(account, thread id)` key and its
/// member rows.
type GroupedThread<'a> = ((&'a str, &'a str), Vec<Arc<MailListRow>>);

/// Conversations grouped by `(account, thread id)`, newest activity first.
fn threaded(rows: &[Arc<MailListRow>], folder: &str, limit: usize) -> Vec<MessageRowView> {
    let mut groups: HashMap<(&str, &str), Vec<Arc<MailListRow>>> = HashMap::new();
    for row in rows {
        let group = (row.account.as_str(), thread_key(row));
        groups.entry(group).or_default().push(Arc::clone(row));
    }
    // Only conversations that touch the folder show; each is ordered by its latest
    // in-folder message, so a follow-up filed elsewhere does not jump it up the list.
    let mut shown: Vec<GroupedThread<'_>> = groups
        .into_iter()
        .filter(|(_, members)| members.iter().any(|row| filed_in(row, folder)))
        .collect();
    shown.sort_by(|(key_a, members_a), (key_b, members_b)| {
        let latest_a = latest_in_folder(members_a, folder);
        let latest_b = latest_in_folder(members_b, folder);
        Reverse(latest_a)
            .cmp(&Reverse(latest_b))
            .then_with(|| key_a.cmp(key_b))
    });
    shown
        .into_iter()
        .take(limit)
        .map(|((_, thread), members)| conversation_row(thread, &members, folder))
        .collect()
}

/// One conversation as a snapshot row: its face is the latest in-folder member, its
/// count every member. A group of one is a message row keyed by that message, the shape
/// a lone (never threaded) message takes too.
fn conversation_row(thread: &str, members: &[Arc<MailListRow>], folder: &str) -> MessageRowView {
    if members.len() == 1 {
        return message_row(&members[0]);
    }
    // The face is the first member in display order (the newest in-folder message),
    // selected with `min_by` over that very ordering.
    let face = members
        .iter()
        .filter(|row| filed_in(row, folder))
        .min_by(|a, b| newest_first(a, b).then_with(|| tiebreak(a, b)))
        .expect("a shown group holds an in-folder member");
    // Unreachable for any group that reached here: the key is a resolved thread id or a
    // message's non-empty provider key, and both parse. Guarded rather than unwrapped
    // so a future key shape degrades to a message row instead of panicking a release
    // build.
    let Ok(thread_id) = ThreadId::try_from(thread) else {
        debug_assert!(false, "a conversation key always parses as a thread id");
        return message_row(face);
    };
    MessageRowView {
        key: RowKey::Thread {
            account: face.account.clone(),
            thread_id,
        },
        account_id: face.account.clone(),
        sender_name: sender_name(&face.mail),
        sender_address: face.mail.from_addr.clone().unwrap_or_default(),
        subject: face.mail.subject.clone().unwrap_or_default(),
        sent_at: face.mail.date_utc.map_or_else(String::new, format_instant),
        unread: members.iter().any(|row| row.mail.flags.is_unread()),
        flagged: members.iter().any(|row| row.mail.flags.flagged()),
        has_attachments: members.iter().any(|row| row.mail.has_attachment),
        thread_count: Some(count(members.len())),
    }
}

/// One message as a snapshot row, keyed by its provider key.
fn message_row(row: &MailListRow) -> MessageRowView {
    MessageRowView {
        key: RowKey::Message {
            account: row.account.clone(),
            key: row.mail.key.clone(),
        },
        account_id: row.account.clone(),
        sender_name: sender_name(&row.mail),
        sender_address: row.mail.from_addr.clone().unwrap_or_default(),
        subject: row.mail.subject.clone().unwrap_or_default(),
        sent_at: row.mail.date_utc.map_or_else(String::new, format_instant),
        unread: row.mail.flags.is_unread(),
        flagged: row.mail.flags.flagged(),
        has_attachments: row.mail.has_attachment,
        thread_count: None,
    }
}

/// Whether `row` is filed in the folder the list is showing. Placement is a separate
/// axis from identity: a provider object can hold several memberships.
fn filed_in(row: &MailListRow, folder: &str) -> bool {
    row.mailboxes.iter().any(|id| id.as_str() == folder)
}

/// The group a row belongs to: its resolved thread id, or its own provider key when the
/// server never threaded it (a lone message is its own conversation).
fn thread_key(row: &MailListRow) -> &str {
    row.mail
        .thread_id
        .as_ref()
        .map_or_else(|| row.mail.key.as_str(), |thread| thread.as_str())
}

/// The newest instant among a conversation's in-folder members; `None` sinks below every
/// dated conversation.
fn latest_in_folder(members: &[Arc<MailListRow>], folder: &str) -> Option<UtcDateTime> {
    members
        .iter()
        .filter(|row| filed_in(row, folder))
        .map(|row| row.mail.date_utc)
        .max()
        .flatten()
}

/// The ordering comparison every sort starts from: newest instant first, undated last.
pub(crate) fn newest_first(a: &MailListRow, b: &MailListRow) -> std::cmp::Ordering {
    Reverse(a.mail.date_utc).cmp(&Reverse(b.mail.date_utc))
}

/// The tiebreak that makes every ordering total: the identity a surface reconciles a row
/// by.
pub(crate) fn tiebreak(a: &MailListRow, b: &MailListRow) -> std::cmp::Ordering {
    (a.account.as_str(), a.mail.key.as_str()).cmp(&(b.account.as_str(), b.mail.key.as_str()))
}

/// The sender's display name, falling back to their address when the header carried no
/// usable name, and empty when there is no sender at all.
fn sender_name(mail: &MailRow) -> String {
    mail.from_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .or(mail.from_addr.as_deref())
        .unwrap_or_default()
        .to_owned()
}

/// Formats a UTC instant as a whole-second `…Z` string a surface localises; the engine's
/// own form can carry sub-second precision, which platform date parsers reject. The
/// reading pane's snapshot formats its instant here too, so a row and the message it
/// opened never carry two spellings of one time.
pub(crate) fn format_instant(instant: UtcDateTime) -> String {
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

/// A member count as the row's `u32`, saturating rather than wrapping.
fn count(members: usize) -> u32 {
    u32::try_from(members).unwrap_or(u32::MAX)
}
