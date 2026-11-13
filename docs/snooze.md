# Snooze: the Astra Mail contract

**Scope.** Setting a message aside until a wake instant: pick a time, the
message moves to a Snooze folder, and at the wake it returns to its origin
folder. Binding on every surface that ships the verb. The engine has no
snooze or scheduled mechanism, so this is a host/core feature: a device-local
snooze store, a host wake timer, and two moves over the existing mail-write
path. No engine change. The gaps are recorded at the end, never left silent.

**Principle.** *The stable identity is the Message-ID, never the provider
key.* An IMAP move synthesizes a new provider key in the destination folder,
so the key stored at snooze time goes stale on the move out. The wake
re-resolves the message by its RFC 5322 `Message-ID` header after a sync; it
never reuses the stored key.

## The store

A new crate `astra-snooze` holds the schedule (the AI ledger store's shape:
one rusqlite connection under a mutex, embedded migrations claimed in a
transaction, the five-second busy timeout, an in-memory open for tests). The
host opens `snooze.sqlite` in the data directory and installs it on the core
(the analytics install shape, a `Mutex<Option<Arc<Store>>>` slot; no
`AppState` field, so the drop order stays as it stands). One row per snoozed
message:

- `account_id`, the owning account (newtyped `AccountId`).
- `message_id`, the RFC 5322 `Message-ID` header (newtyped `MessageIdHeader`),
  the stable identity the wake re-resolves by.
- `provider_key`, the key at snooze time, diagnostic only: it goes stale on
  the move and is never the move-back target.
- `wake_at`, the wake instant in unix seconds.
- `origin_folder`, the folder key the message returns to.
- `subject` and `sender`, the re-resolution's fallback beside the Message-ID.
- `created_at`, the store's own stamp; an index on `wake_at`.

The methods are `record`, `due(now)` (`wake_at <= now`, earliest first),
`remove`, and `list` (for a future Snoozed view). A row whose identity does
not parse is skipped, never fatal to the batch reading it. The store is
device-local: it does not roam across devices, and the CLI does not share it
(a host-timer feature).

## The core snooze and wake

`snooze_message(row, wake_at)` refuses a conversation row (a move names
messages only), ensures the account's Snooze folder exists, reads the
message's `Message-ID` and origin folder from the store's own copy, records
the entry, then dispatches `MoveMessages` into the Snooze folder. A message
with no `Message-ID` is refused: the stable identity is what the wake needs.

`wake_due_snoozes(now)` reads the due entries and, for each, syncs the account
so the moved copy reconciles, re-resolves the message by `Message-ID` within
the Snooze folder (subject and sender as fallback), dispatches `MoveMessages`
back to the origin folder (or the Inbox when the origin is gone), and removes
the entry. A message or folder that no longer resolves is a stale entry: it
is removed without a move, never stranding the batch. The move republishes
`Surface::MailboxList`, so the web learns of the wake through the existing
`surface_changed` loop; no new event ships.

## The Snooze folder

There is no Snooze `MailboxRole`. Snooze creates a folder named "Snooze" per
account (`FolderIntent::Create`) when absent and resolves its key by the
conventional name (the leaf, case-insensitively). The folder is reachable via
the pane; a dedicated Snoozed view is a follow-up.

## The wake timer

A host `snooze_timer` (the calendar timer's shape) ticks every minute, reads
the store's due entries, and calls the wake body through `AppPort`. It is
stateless over the store, so a restart re-reads the due entries. The interval
and tick limit are debug-overridable (`ASTRA_SNOOZE_SECONDS`,
`ASTRA_SNOOZE_LIMIT`) for deterministic runs; a release build holds the
standing cadence and never stops. The timer holds the two `Arc`s (the core
and the store), so the `AppState` drop order is untouched.

## The surface

The row menu's "Snooze…" entry (a single message; a conversation or a batch
stands disabled with its reason) opens a picker offering later today,
tomorrow, and next week as computed instants. Choosing one fires the host's
`snooze_message` command and closes; Cancel and Escape dispatch nothing. The
intent constructor and wire types ride `services/snooze.ts` (never `ipc.ts`,
at the cap); the desktop command lives in a `commands/snooze.rs` submodule
(`commands.rs` is at the cap).

## Closed-app wake precision

The timer runs only while the process lives. A wake due while the app is
closed fires at the next launch, not the exact instant: inherent to a desktop
host timer, recorded not hidden.

## Known gaps

- **Device-local**: the snooze store does not roam across devices.
- **Closed-app wake precision**: a wake due during a closed app fires at next
  launch, not the exact instant.
- **Custom wake time** is a fast-follow (the native datetime-local reuse
  exists).
- **A dedicated Snoozed view/filter** is a follow-up (the Snooze folder is
  reachable via the pane).
- **Conversation snooze, re-snooze/undo, and a CLI/agent snooze verb** are
  follow-ups.
- **A message with no Message-ID** cannot be snoozed: the stable identity is
  what the wake re-resolves by.