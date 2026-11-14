# Watch-replies design

**Status.** Decided 2026-11-14. Slices branch from main at this commit.

**Scope.** There is no way to be told when a conversation you care about gets a
reply. This kylins addition adds watch-replies: watch a conversation, and the first
reply raises a "Reply received" notification and clears the watch (one-shot). The
watch NEVER moves mail and owns no engine state (the locked kylins reference red
line, watch_replies.rs). A device-local (account, thread) membership store, a
post-pass arrival hook that resolves threads through the engine, a reply notice
through the bridge, a watch_replies_enabled preference (default on). No engine
change; no eval/pipeline bump.

## The crux: derived threads re-key on merge

A derived thread is named the SMALLEST owned Message-ID in the merged component
(vendor/engine store-sqlite derived_ops/threading.rs); when a reply arrives with a
smaller Message-ID, the engine re-keys every stored member onto the new name. A
watch keyed on the old thread id, matched against the reply's freshly resolved
thread id, MISSES (for random Message-IDs the reply sorts first roughly half the
time). The watch therefore carries one representative member_key (a stable provider
key captured at set time, the reminder's provider_key category, a link not engine
state), and the check resolves BOTH sides through the engine at check time: the
batch's notice keys AND every watch's member key, one mail_by_keys per account,
matched resolved-against-resolved. The member row survives re-keying (its thread_id
moves with the component), so both sides always name the current truth. This
member-key re-resolution is in v1, not a follow-up (without it the feature fails on
roughly half of first replies over plain IMAP). A re-keying regression test (a reply
whose Message-ID sorts BEFORE the original's) pins it.

## D1: the watch store

A new crate crates/astra-watches (the astra-reminders shape: rusqlite under a Mutex,
embedded migrations, open with the 5s busy timeout, open_in_memory). One table:
watch_replies (account_id, thread_id, member_key, created_at, PRIMARY KEY
(account_id, thread_id)). member_key is one representative provider key of the
watched conversation captured at set time. Methods: watch (idempotent upsert,
answers inserted), unwatch (DELETE, answers removed; the one-shot consume rides this
exact method), list (rowid order, for a future settings surface). No due logic, no
timer (the arrival lane is the only trigger). Malformed identity rows skipped, never
fatal. Device-local; the CLI does not share it.

## D2: the core watch verbs + the post-pass check

A core install_watch_store (the install_reminder_store shape, a
Mutex<Option<Arc<Store>>> slot; lib.rs at the cap, reclaim doc lines) and a new
watch_replies.rs: watch_thread(RowRef) / unwatch_thread(RowRef) (resolve the thread
key like categories.rs resolve_thread_key, plus member-key capture: a thread row
reads mail_on_threads for the newest member, a message row uses its own key),
list_watches, and check_watched_replies(batch) -> Vec<NewMailNotice> (async: the
watch_replies_enabled preference gate [off -> empty, consume nothing], group the
batch keys by account, one mail_by_keys per account for the batch keys + the watch
member keys, map each to thread_key, match resolved-against-resolved, build a
NewMailNotice from the first reply notice per hit, consume via unwatch, fail-open on
any read/store failure [logged, never stranding the pass or the base notice]).

A watch_replies_enabled preference (default on, the notifications_enabled pattern:
the preferences field + state projection + persist leg, a SetWatchRepliesEnabled
SettingsIntent arm [settings/mod.rs at the cap, reclaim], the SettingsBlock wire
field).

## D3: the host hook + the reply notice + the surface

The boot installs the watch store (the categories pattern: open watches.sqlite +
install_watch_store in assemble_state, no timer, no AppState field, the drop order
untouched). A WatchPort supertrait (the CategoriesPort pattern, trait AppPort:
WatchPort, each impl's block in its own child file, E0119-safe; app_state.rs at the
cap). New commands/watch_replies.rs (watch_set/watch_clear/watch_list, the
commands/reminders.rs shape; commands.rs at the cap). The production HostDelivery::
pass runs check_watched_replies after the drain (it is async, holds the notify Arc;
the sync NewMailQueue commit fold is untouched), gated on notifications_enabled, and
delivers the answer through a new NotifyBridge::deliver_replies.

deliver_replies (the deliver_reminders shape): one OS notification per hit titled
"Reply received", body the sender/subject, NO focus suppression (an explicit user
request, the reminder precedent; Astra has no DND), history emit on a new reply
event. The notice carries the reply's own account/key/from/subject/preview so
click-to-open works unchanged; the base "New mail" notice still fires (two
announcements, kylins parity). The web replyFlow/replyEvents (the reminderFlow
clone) record into the same notificationHistoryStore.

The web: services/watchReplies.ts (the invoke wrappers, never ipc.ts), a zustand
watch-set store (seeded from watch_list, refreshed on toggle), a watchMenu.ts item
("Watch replies"/"Unwatch", single row, batch disabled with reason, the
blockSenderMenu pattern; rowMenu.ts at the cap), a General-settings watch_replies_
enabled toggle.

## D4: the contract

A new docs/watch-replies.md (the red line, the one-shot lifetime, the member-key
re-resolution mechanism and WHY [derived thread re-keying], the escalated deviations,
the recorded gaps) + zh; the capabilities row; a changelog fragment; the
porting-backlog shipped-list move. The zh mirrors move in the same commit.

## Reference deviations, escalated (AGENTS.md)

- No focus suppression on the reply notice + consume-at-drain: kylins runs the watch
  hook only when the base toast fires (behind the DND/mute gates) and consumes there;
  this slice follows the shipped reminder contract (no focus suppression, consume at
  drain so a focused window still consumes honestly). Escalated, not silent.

## Known gaps recorded, not hidden

- Overflow folding: a reply among more than 5 qualifying arrivals in one pass folds
  into the overflow count; its key never reaches the drained batch, so the check
  misses it and the watch stays armed until a later pass (the queue carrying the
  pass's full new-key set is the upgrade path).
- Background-lane only: the check rides the delivery driver (the sole production
  drain); manual-refresh arrivals are checked at the next background pass (the base
  new-mail notice's same exposure).
- Stale member link: if the representative watched message is moved (IMAP synthesizes
  a new key) or deleted, the watch can no longer resolve; fail-open, watch stays
  armed but silent (the snooze Message-ID re-resolution is the upgrade path).
- Device-local, no roam, no CLI/agent verb, no badge, no settings/debug list
  (optional follow-up), no multi-select watch (kylins parity).

## Test-first

Every behaviour lands with its red. Store tests (one-shot/idempotence/list-order,
account scoping, migration race). Core hook tests over the dispatch fakes (script an
original with a Message-ID, watch it, deliver a reply with In-Reply-To set; the
in-memory engine derives the thread so the hook resolves for real; the RE-KEYING
REGRESSION: a reply whose Message-ID sorts BEFORE the original's still fires; the
preference off -> no notice + watch NOT consumed; unwatched -> nothing; second reply
after consume -> nothing; fail-open on a failed resolve; the provider-threaded leg).
A notify-bridge test (deliver_replies honours enabled, no focus suppression, the
reply event). Web tests (the menu label flip + batch disabled, the service, the
settings toggle, the reply-event mount). ONE gated live harness proof
(watch_replies_proof.rs: submit an original, watch the thread via the core, deliver a
reply with In-Reply-To set [choose the reply's Message-ID to sort first, proving
re-key survival over the real wire], run the pass + check, assert one "Reply
received" notice naming the reply + the watch row gone + a second reply raises
nothing + both messages still in INBOX unmoved, withdraw). No live model proof; the
offline battery is the primary proof.