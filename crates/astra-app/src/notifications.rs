// SPDX-License-Identifier: GPL-3.0-only
//! The new-mail notification queue: the core surface a pass's commits feed
//! and the host bridge drains. One row qualifies when it is **new** (no
//! prior key in the store), lands in an inbox-class folder, and is not the
//! account's own filing of a message it sent; everything else a commit
//! carries (a flag change, a moved copy, the sent filing) is state the
//! person already knows.
//!
//! The new-versus-changed call is honest by construction rather than by
//! timing: the engine reports a commit only after it has applied, so a
//! store lookup at commit time would always find the row it just wrote.
//! [`NewMailQueue::arm`] therefore seeds each account's known keys from the
//! store **before** its pass runs, and the commits of that pass (and every
//! later one) classify against the seed plus the keys the queue itself has
//! watched land. The one inaccuracy left is a row the store gains outside a
//! sync commit between the seed and the pass presenting it, which in this
//! app is a local move the person just made.
//!
//! The queue is bounded: it holds a handful of notices and folds everything
//! beyond into a count, so a pass landing a folder's worth of mail cannot
//! grow memory or, once the host drains it, a notification storm.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Mutex,
};

use engine_api::{
    AccountId, EmailAddress, Engine, MailboxId, MailboxRole, Message, ProviderKey, SyncCommit,
};

/// How many notices the queue holds at once; rows a pass delivers beyond
/// this fold into the batch's overflow count, which the host's summary
/// notification answers.
pub(crate) const NEW_MAIL_QUEUE_CAP: usize = 5;

/// One message the queue holds for the host bridge to notify: who it is
/// from, what it is about, and how it begins, beside the ids a host needs
/// to open the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMailNotice {
    /// The account the message landed in.
    pub account: AccountId,
    /// The message's provider key, so the host can open the row.
    pub key: ProviderKey,
    /// The first `From` address, when the envelope carried one.
    pub from: Option<EmailAddress>,
    /// The subject, when the envelope carried one.
    pub subject: Option<String>,
    /// The list snippet: how the message begins.
    pub preview: Option<String>,
}

/// One drained batch: the notices the queue holds, and how many further
/// qualifying rows arrived beyond its bound.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewMailBatch {
    /// The notices to show, oldest first.
    pub messages: Vec<NewMailNotice>,
    /// How many more qualifying rows the queue folded away.
    pub overflow: usize,
}

/// One armed account: the facts its pass's commits classify against.
struct Armed {
    /// The address the account sends as, so its own filings stay silent.
    identity: EmailAddress,
    /// The account's inbox-class folders, by id.
    inbox: BTreeSet<MailboxId>,
    /// Every key the account held at its first arming plus every key the
    /// queue has watched land since; a commit's row is new exactly when its
    /// key is not in this set.
    known: BTreeSet<ProviderKey>,
    /// Whether this pass is the account's first import of a cold store,
    /// decided at arming: history arriving for the first time is not new
    /// mail. Recomputed at every arming, so the pass that follows an import
    /// classifies normally.
    importing: bool,
}

/// The shared state behind the queue.
struct QueueState {
    /// The notices held, oldest first, never longer than [`NEW_MAIL_QUEUE_CAP`].
    rows: VecDeque<NewMailNotice>,
    /// Qualifying rows beyond the queue's bound.
    overflow: usize,
    /// The armed accounts, by id; a commit for an unarmed account is
    /// ignored, because newness cannot be judged without the seed.
    armed: BTreeMap<AccountId, Armed>,
}

/// The new-mail queue: armed per account before a pass, fed by the pass's
/// commits through the [`SyncObserver`](engine_api::SyncObserver) fold, and
/// drained by the host bridge after the pass.
pub(crate) struct NewMailQueue {
    state: Mutex<QueueState>,
}

impl NewMailQueue {
    /// An empty, unarmed queue.
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(QueueState {
                rows: VecDeque::new(),
                overflow: 0,
                armed: BTreeMap::new(),
            }),
        }
    }

    /// Arms `account`'s next pass: refreshes the inbox-class folder set and
    /// the sending identity, and seeds the account's known keys from the
    /// store the first time it is armed in this process. The seed is the
    /// honest "prior key" test the engine's post-commit observer cannot run
    /// for itself; from here on the queue maintains the set as commits land.
    pub(crate) async fn arm(&self, engine: &Engine, account: &AccountId, identity: &EmailAddress) {
        let seeded = {
            let state = self.state.lock().expect("new-mail mutex poisoned");
            state.armed.contains_key(account)
        };
        // The folder list is re-read every pass: an account whose tree moved
        // (a renamed or re-created inbox) re-classifies against the truth.
        let mailboxes = engine.mailboxes(account).await.unwrap_or_default();
        let inbox: BTreeSet<MailboxId> = mailboxes
            .into_iter()
            .filter(|mailbox| mailbox.role == Some(MailboxRole::Inbox))
            .map(|mailbox| mailbox.id)
            .collect();
        // The key seed is read once per process per account: the whole
        // mailbox's keys, the rows the store held before this process ever
        // judged a commit. Later passes keep the set true by recording every
        // commit's keys, which is cheaper than re-reading the store per pass
        // and fresher than a set that forgot what landed yesterday.
        let known = if seeded {
            BTreeSet::new()
        } else {
            engine
                .mail_window(std::slice::from_ref(account), usize::MAX)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|row| row.mail.key)
                .collect()
        };
        let mut state = self.state.lock().expect("new-mail mutex poisoned");
        match state.armed.get_mut(account) {
            Some(armed) => {
                armed.identity = identity.clone();
                armed.inbox = inbox;
                // The known set the last pass maintained answers whether
                // this pass imports (still nothing held) or classifies.
                armed.importing = armed.known.is_empty();
            }
            None => {
                state.armed.insert(
                    account.clone(),
                    Armed {
                        identity: identity.clone(),
                        inbox,
                        importing: known.is_empty(),
                        known,
                    },
                );
            }
        }
    }

    /// Classifies one commit's upserted rows against the armed account:
    /// each row that is new, inbox-class, and not the account's own filing
    /// becomes a notice; every row's key joins the known set whether it
    /// qualified or not, so the next pass sees it as the change it is.
    pub(crate) fn committed(&self, commit: &SyncCommit<'_>) {
        let account = commit.scope.account().clone();
        let mut state = self.state.lock().expect("new-mail mutex poisoned");
        // Destructured so the armed entry and the queue itself borrow
        // disjoint halves of the one state.
        let crate::notifications::QueueState {
            rows,
            overflow,
            armed,
        } = &mut *state;
        let Some(armed) = armed.get_mut(&account) else {
            return;
        };
        // A store that held nothing when the pass armed is being imported,
        // not delivered to: the pass's whole mailbox is history, and the
        // keys it records here are what turns the next pass honest.
        let importing = armed.importing;
        for message in commit.upserted {
            let key = message.id.key().clone();
            let is_new = !armed.known.contains(&key);
            armed.known.insert(key.clone());
            if !is_new || importing || !qualifies(message, armed) {
                continue;
            }
            if rows.len() < NEW_MAIL_QUEUE_CAP {
                rows.push_back(NewMailNotice {
                    account: account.clone(),
                    key,
                    from: message.envelope.from.first().cloned(),
                    subject: message.envelope.subject.clone(),
                    preview: message.preview.clone(),
                });
            } else {
                *overflow += 1;
            }
        }
    }

    /// Takes the whole batch and leaves the queue empty: the host bridge's
    /// drain, called once a pass has settled.
    pub(crate) fn drain(&self) -> NewMailBatch {
        let mut state = self.state.lock().expect("new-mail mutex poisoned");
        NewMailBatch {
            messages: state.rows.drain(..).collect(),
            overflow: std::mem::take(&mut state.overflow),
        }
    }
}

/// Whether `message` is one the host should hear about: filed in one of the
/// account's inbox-class folders, and sent by someone other than the
/// account's own identity (the sent filing a provider presents in the
/// Inbox, the way a label-shaped server does).
fn qualifies(message: &Message, armed: &Armed) -> bool {
    let inbox_filed = message
        .mailboxes
        .iter()
        .any(|mailbox| armed.inbox.contains(mailbox));
    let own_filing = message
        .envelope
        .from
        .iter()
        .any(|from| from.email.eq_ignore_ascii_case(&armed.identity.email));
    inbox_filed && !own_filing
}

impl<P> crate::App<P> {
    /// Takes the new-mail batch the passes have collected and leaves the
    /// queue empty: the drain the host bridge calls once a pass has
    /// settled, so the OS notification policy (one per message unfocused, a
    /// summary past a few, none while the window is focused) lives in the
    /// host where the window state is.
    pub fn drain_new_mail(&self) -> NewMailBatch {
        self.new_mail.drain()
    }
}

#[cfg(test)]
#[path = "notifications_tests.rs"]
mod tests;
