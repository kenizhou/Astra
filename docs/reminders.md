# Flag reminders: the Astra Mail contract

**Scope.** Asking to be reminded about a message later: pick a wake time on
a message, and at the wake time the user is notified. The message does NOT
move and is NOT auto-flagged (the existing Flag verb stays separate).
Binding on every surface that ships the verb. The engine has no reminder
mechanism, so this is a host/core feature: a device-local reminder store, a
host wake timer, and a reminder notification event. No engine change. The
gaps are recorded at the end, never left silent.

**Principle.** *The message stays put, so the provider key stays valid.* A
reminder never moves mail and never touches unread counts; it owns no engine
state. Because the message never moves, the provider key recorded at set
time stays a valid link, and the wake needs no sync and no Message-ID
re-resolution (the snooze store's re-resolution machinery is absent here by
design). The subject and sender are captured at set time so the wake builds
its notice without an engine read.

## The store

A new crate `astra-reminders` holds the schedule (the AI ledger store's
shape: one rusqlite connection under a mutex, embedded migrations claimed in
a transaction, the five-second busy timeout, an in-memory open for tests).
The host opens `reminders.sqlite` in the data directory and installs it on
the core (the analytics install shape, a `Mutex<Option<Arc<Store>>>` slot;
no `AppState` field, so the drop order stays as it stands). One row per
reminder:

- `account_id`, the owning account (newtyped `AccountId`).
- `provider_key`, the key at set time (newtyped `ProviderKey`), the link the
  wake opens the row by; it stays valid because the message never moves.
- `wake_at`, the wake instant in unix seconds.
- `subject` and `sender`, captured at set time for the wake's notice.
- `created_at`, the store's own stamp; an index on `wake_at`.

There is no `origin_folder` and no `message_id`: neither applies, because
the message never moves. The methods are `record`, `due(now)`
(`wake_at <= now`, earliest first), `remove`, and `list`. A row whose
identity does not parse is skipped, never fatal to the batch reading it. The
store is device-local: it does not roam across devices, and the CLI does not
share it (a host-timer feature, the snooze precedent).

## The core remind and wake

`remind_message(row, wake_at)` refuses a conversation row (a reminder names
one message), reads the message's provider key, subject, and sender from the
engine's own copy, and records the entry. NO move, NO flag edit.

`wake_due_reminders(now)` reads the due entries and, for each, builds a
`NewMailNotice` (the account, the provider key, and the subject and sender
captured at set time, reusing the new-mail payload so the wire shape is
identical), removes the entry, and answers the notices. No sync, no
re-resolution, no move, no surface republication. A bad row is skipped by
the due read, never stranding the batch. The body is synchronous (it reads
the store and builds notices, no engine call); the host's port wraps it in
the async tick the timer drives.

## The wake timer

A host `reminder_timer` (the snooze timer's shape) ticks every minute, reads
the store's due entries, and calls the wake body through `AppPort`,
delivering the notices through the notification bridge. It is stateless over
the store, so a restart re-reads the due entries. The interval and tick limit
are debug-overridable (`ASTRA_REMINDER_SECONDS`, `ASTRA_REMINDER_LIMIT`) for
deterministic runs; a release build holds the standing cadence and never
stops. The timer holds the three `Arc`s (the core, the store, and the
bridge), so the `AppState` drop order is untouched. The bridge is built once
by the caller that owns the window handle and passed into `assemble_state`
(the windowed host builds `windowed`, the headless host `logged`), shared
with the delivery loop.

## The reminder notification

`NotifyBridge::deliver_reminders(batch, enabled)` honours the
`notifications_enabled` switch, shows one OS notification per due reminder
titled "Reminder" (the body the captured subject), and emits the
`NewMailBatch` payload (overflow zero) on the `reminder` event. There is NO
focus suppression: an explicit user request fires while the window is
focused, and the enabled switch is the only gate (Astra has no DND setting).
A history channel failure is logged, never raised, so it cannot suppress the
OS notification that already crossed. The web's `reminderFlow` and
`reminderEvents` record each landing into the SAME notification history the
new-mail mount feeds; the history panel needs no change (entries carry
account and key, so click-to-open works unchanged).

## The surface

The row menu's "Remind me…" entry (a single message; a conversation or a
batch stands disabled with its reason) opens a picker offering later today,
tomorrow, and next week as computed instants. The picker REUSES the snooze
preset math (`snoozeChoices` from `services/snooze.ts`); the labels read
correctly for reminders. Choosing one fires the host's `remind_message`
command and closes; Cancel and Escape dispatch nothing. The wire rides
`services/reminders.ts` (never `ipc.ts`, at the cap); the desktop command
lives in a `commands/reminders.rs` submodule (`commands.rs` is at the cap).

## Reference deviation, escalated

The locked kylins reference (the Follow Up family) keeps a due flag
PERSISTENT: the row stays until the user clears it, a badge marks the
thread, and a once-only frontend gate fires the toast once. This slice ships
the SMALLER remove-on-wake lifetime (the snooze precedent): the entry is
removed when it fires. A persistent row with no badge surface is invisible
dead weight; the badge surface that makes persistence meaningful is a second
slice. This is a deliberate deviation from the reference lifetime, escalated
here and in the PR, not silent. The reference's red line IS honoured:
reminders never move mail, never touch unread counts, own no engine state.

## Closed-app wake precision

The timer runs only while the process lives. A wake due while the app is
closed fires at the next launch, not the exact instant: inherent to a desktop
host timer, recorded not hidden.

## Known gaps

- **The kylins-parity lifetime** (the persistent due mark and badge, a Clear
  reminder verb, upsert re-set so re-flagging moves the deadline, and the
  once-only toast gate) is the escalated deviation's follow-up.
- **Device-local**: the reminder store does not roam across devices.
- **Closed-app wake precision**: a wake due during a closed app fires at next
  launch, not the exact instant.
- **Custom wake time**, conversation and batch reminders, a ribbon or palette
  entry, and a CLI/agent verb are follow-ups.
- **A stale open-link** if the message is moved or deleted before the wake
  (no Message-ID re-resolution in v1; the same exposure new-mail notices
  carry).
- **No summary folding** when many reminders come due at once; no DND
  setting.
