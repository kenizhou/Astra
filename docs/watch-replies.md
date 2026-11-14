# Watch-replies: the Astra Mail contract

**Scope.** Watching a conversation for its first reply: watch a conversation,
and the first reply that lands raises a "Reply received" notification and
clears the watch (one-shot). Binding on every surface that ships the verb. The
engine has no watch concept, so this is a host/core feature: a device-local
watch store, a post-pass arrival check, and a reply notification event. No
engine change. The gaps are recorded at the end, never left silent.

**Principle (the locked kylins reference red line).** *A watch NEVER moves mail
and owns no engine state.* The watch is a plain membership mark from set until
the first reply clears it. It writes nothing to the engine: no move, no flag,
no folder, no keyword. The check resolves threads through engine READS only.

## The crux: derived threads re-key on merge

A derived thread is named the SMALLEST owned Message-ID in its merged
component (the engine's `store-sqlite` derived threading, mirrored by the
in-memory store). When a reply arrives owning a smaller Message-ID than the
original, the engine merges the two into one component and RE-KEYS every
stored member onto the reply's smaller id. A watch keyed on the thread id
captured at set time, matched against the reply's freshly resolved thread id,
would therefore MISS roughly half of first replies over plain IMAP (for random
Message-IDs the reply sorts first about half the time).

The watch therefore carries one representative `member_key`: a stable provider
key of the watched conversation captured at set time (a link, not engine
state). The check resolves BOTH sides through the engine at check time, the
batch's notice keys AND every watch's member key, one `mail_by_keys` per
account, and matches resolved-against-resolved. The member row survives
re-keying (its thread id moves with the component), so both sides always name
the current truth. This member-key re-resolution is in v1, not a follow-up; a
regression test (a reply whose Message-ID sorts BEFORE the original's) pins it,
and the gated live proof chooses the reply's Message-ID to sort first so the
re-key is proven over the real wire.

## The store

A new crate `astra-watches` holds the membership (the reminder store's shape:
one rusqlite connection under a mutex, embedded migrations claimed in a
transaction, the five-second busy timeout, an in-memory open for tests). The
host opens `watches.sqlite` in the data directory and installs it on the core
(the categories install shape, a `Mutex<Option<Arc<Store>>>` slot; no timer and
no `AppState` field, so the drop order stays as it stands). One row per watched
conversation:

- `account_id`, the owning account (newtyped `AccountId`).
- `thread_id`, the thread id resolved at set time, the row's handle within the
  account (a plain identifier, the `ThreadRef` convention).
- `member_key`, one representative provider key captured at set time
  (newtyped `ProviderKey`), the link the check re-resolves by.
- `created_at`, the store's own stamp.
- `PRIMARY KEY (account_id, thread_id)`.

The methods are `watch` (idempotent upsert on the key, answers inserted; a
re-watch keeps the standing row and its member link), `unwatch` (DELETE,
answers removed; the one-shot consume rides this exact method), and `list`
(rowid order across accounts, for a future settings surface). There is no due
logic and no timer: the arrival lane is the only trigger. A row whose identity
does not parse is skipped, never fatal to the listing. The store is
device-local: it does not roam across devices, and the CLI does not share it.

## The core verbs and the post-pass check

`watch_thread(row)` resolves the thread key (a thread row by its id directly, a
message row through one engine read, the categories `resolve_thread_key`
convention) and captures one representative member key: a thread row reads its
newest member through `mail_on_threads`, a message row uses its own key. It
records the mark and answers whether this call inserted. NO move, NO engine
write. `unwatch_thread(row)` resolves the same key and clears the mark.
`list_watches()` answers every watch, insertion order.

`check_watched_replies(batch)` is the async post-pass body. The
`watch_replies_enabled` preference gates it (off answers empty and consumes
nothing). It groups the batch's notice keys by account, reads one
`mail_by_keys` per account for the batch keys plus the watch member keys, maps
each resolved row to its thread key (the resolved thread id, or the provider
key when never threaded), and matches resolved-against-resolved. For each watch
a notice replies to, it builds a `NewMailNotice` from the first matching reply
(the reply's own account, key, from, subject, and preview, so click-to-open
works unchanged) and consumes the watch through `unwatch`. One notice per
distinct reply even when several watches match it. Fail-open: any read or store
failure is logged and skips its account, never stranding the pass or the base
new-mail notice; a watch whose member key no longer resolves stays armed but
silent.

## The host hook and the reply notification

The production `HostDelivery::pass` runs the check after the drain (the sync
`NewMailQueue` commit fold is untouched), gated on `notifications_enabled`, and
delivers the answer through `NotifyBridge::deliver_replies`. The check rides
the delivery driver, the sole production drain.

`deliver_replies(batch, enabled)` honours the `notifications_enabled` switch,
shows one OS notification per reply titled "Reply received" (the body the
sender and subject), and emits the `NewMailBatch` payload (overflow zero) on
the `reply` event. There is NO focus suppression: an explicit user request
fires while the window is focused, and the enabled switch is the only gate
(Astra has no DND setting). A history channel failure is logged, never raised,
so it cannot suppress the OS notification that already crossed. The base "New
mail" notice still fires beside it (two announcements, kylins parity). The
web's `replyFlow` and `replyEvents` record each landing into the SAME
notification history the new-mail mount feeds; the history panel needs no
change (entries carry account and key, so click-to-open works unchanged).

## The surface

The row menu's "Watch replies" entry (a single row; a batch stands disabled
with its reason) flips to "Unwatch" when the conversation stands watched. The
wire rides `services/watchReplies.ts` (never `ipc.ts`, at the cap); a zustand
watch-set store seeds from `watch_list` and refreshes on every toggle, keying
by account and id (each watch's thread id and representative member key, so a
thread row resolves by its thread id and a message row by the member key the
core captured). The desktop commands live in a `commands/watch_replies.rs`
submodule (`commands.rs` is at the cap), behind a `WatchPort` supertrait (the
`CategoriesPort` pattern, each impl's block in its own child file). The General
settings page carries a `watch_replies_enabled` toggle (on by default, the
`notifications_enabled` pattern end to end).

## Reference deviations, escalated

The locked kylins reference runs the watch hook only when the base toast fires
(behind the DND/mute gates) and consumes there. This slice follows the shipped
reminder contract instead: NO focus suppression on the reply notice, and
consume at drain so a focused window still consumes honestly. Both are
deliberate deviations from the reference, escalated here and in the PR, not
silent. The reference's red line IS honoured: watches never move mail and own
no engine state.

## Known gaps

- **Overflow folding**: a reply among more than five qualifying arrivals in one
  pass folds into the overflow count; its key never reaches the drained batch,
  so the check misses it and the watch stays armed until a later pass (the
  queue carrying the pass's full new-key set is the upgrade path).
- **Background-lane only**: the check rides the delivery driver (the sole
  production drain); manual-refresh arrivals are checked at the next background
  pass (the base new-mail notice's same exposure).
- **Stale member link**: if the representative watched message is moved (IMAP
  synthesizes a new key) or deleted, the watch can no longer resolve; fail-open,
  the watch stays armed but silent (the snooze Message-ID re-resolution is the
  upgrade path).
- **The menu label resolves by the representative member**: a message row that
  is not the watched representative member reads unwatched even when its
  conversation stands watched (the thread row and the representative message
  resolve correctly); a row-level watched badge is the upgrade path.
- **Device-local**: no roam, no CLI/agent verb, no badge, no settings/debug
  list (optional follow-up), no multi-select watch (kylins parity).