# Flag reminders design

**Status.** Decided 2026-11-14. Slices branch from main at this commit.

**Scope.** There is no way to ask to be reminded about a message later. This
kylins addition adds flag reminders: pick a wake time on a message, and at the
wake time the user is notified. The message does NOT move and is NOT auto-flagged
(the existing Flag verb stays separate). It reuses the snooze host-timer shape
but with a simpler wake (no move, no Message-ID re-resolution: the message stays
put, so the provider key recorded at set time stays a valid link). A new
astra-reminders store, a host reminder timer, a reminder notification event. No
engine change; no eval/pipeline bump.

## Reference deviation, escalated (AGENTS.md)

The locked kylins reference (thread_flags.rs, followups.rs, the Follow Up family)
keeps a due flag PERSISTENT: the row stays until the user clears it, a badge marks
the thread, and a once-only frontend gate fires the toast once. This slice ships
the SMALLER remove-on-wake lifetime (the snooze precedent): the entry is removed
when it fires. Rationale: a persistent row with no badge surface is invisible dead
weight; the badge surface that makes persistence meaningful is a second slice.
This is a deliberate deviation from the reference lifetime, escalated here and in
the PR, not silent. The kylins-parity items are named follow-ups: the persistent
due mark + badge, a Clear reminder verb, upsert re-set (re-flag moves the
deadline), and the once-only toast gate. The reference's red line IS honoured:
flags never move mail, never touch unread counts, own no engine state.

## D1: the reminder store

A new crate crates/astra-reminders (copying astra-snooze's shape, NOT its table:
snooze's origin_folder is NOT NULL and its wake moves mail, neither applies here).
One row per reminder: id, created_at, account_id, provider_key, wake_at (unix
seconds, indexed), subject, sender. No origin_folder, no message_id in v1 (the
message never moves, so the provider key stays valid; the subject/sender are
captured at set time so the wake needs no engine read). Methods record / due(now)
(wake_at <= now) / list / remove, the same migration claim and malformed-skip
discipline. Newtyped identities (AccountId, ProviderKey). Device-local; the CLI
does not share it (a host-timer feature, the snooze precedent).

## D2: the core remind + wake

A core install_reminder_store (the install_snooze_store shape, a
Mutex<Option<Arc<Store>>> slot; lib.rs at the cap, reclaim the few doc lines) and:

- remind_message(account, row, wake_at): refuse a conversation row (the snooze
  precedent), read the message's key + subject + sender from the engine's own
  copy, record the entry. NO move, NO flag edit.
- wake_due_reminders(now) -> Vec<NewMailNotice>: for each due entry build a
  NewMailNotice (account, key, subject, sender captured at set time, reusing the
  new-mail payload so the wire shape is identical), remove the entry, answer the
  notices. No sync, no re-resolution, no move, no surface republication. A bad row
  is skipped/removed, never strands the batch.

## D3: the host timer + the reminder notification

A host reminder_timer.rs copying snooze_timer.rs (ReminderHost wait/now/due/wake
seams, a 60s tick, ASTRA_REMINDER_SECONDS/LIMIT debug overrides, spawn holds the
Arcs, stateless over the store so a restart re-reads due entries; a wake due while
the app was closed fires at next launch). The one structural difference from
snooze: the reminder timer needs a NotifyBridge, which assemble_state cannot build
(no AppHandle), so pass Arc<NotifyBridge> into assemble_state (the windowed caller
has the handle; headless builds logged()), shared with delivery.

A new NotifyBridge::deliver_reminders(batch, enabled) + REMINDER_EVENT = "reminder"
(notify.rs has room): honour notifications_enabled, title "Reminder", one
notification per due reminder, NO focus suppression (an explicit user request
fires while the window is focused; the enabled switch is the only gate, Astra has
no DND setting), emit the NewMailBatch payload (overflow 0). The web gains
events/reminderFlow.ts + reminderEvents.ts (near-copies of the new-mail pair)
recording into the SAME notificationHistoryStore (the history panel needs no
change: entries carry account + key, click-to-open works unchanged).

## D4: the surface

A row-menu "Remind me…" entry (single message, the snoozeItem shape; conversation
disabled with reason) opening a new features/reminders/ picker (reminderStore.ts +
ReminderDialog.tsx, the SnoozeDialog pattern, useDialogFocus). The picker REUSES
snoozeChoices from services/snooze.ts (the pure exported helper; the labels read
correctly for reminders, the kylins reference reuses the snooze preset math too).
The wire rides a new services/reminders.ts (remindMessage invoking remind_message;
never ipc.ts, at the cap). A new commands/reminders.rs submodule (commands.rs at
the cap, reclaim a doc line), registered in lib.rs. AppPort gains the two methods
(app_state.rs near the cap, reclaim doc lines; no AppState field, core-install).

## D5: the contract

A new docs/reminders.md (the store, the no-move wake, the reminder event, the
no-focus-suppression policy, the escalated remove-on-wake deviation, the recorded
gaps) + zh; the capabilities row beside snooze; a changelog fragment; the
porting-backlog shipped-list move. The zh mirrors move in the same commit.

## Known gaps recorded, not hidden

- The kylins-parity lifetime (persistent due mark + badge, Clear reminder, upsert
  re-set, once-only toast gate) is the escalated deviation's follow-up.
- Custom wake time; conversation/batch reminders; a ribbon/palette entry; a
  CLI/agent verb.
- Device-local, no cross-device roam; closed-app wake fires at next launch.
- A stale open-link if the message is moved/deleted before wake (no Message-ID
  re-resolution in v1; the same exposure new-mail notices carry).
- No summary folding when many reminders come due at once; no DND setting.

## Test-first

Every behaviour lands with its red. Store tests (record/due/remove round-trip, the
due boundary, the migration claim, malformed-skip, concurrent first opens) over
open_in_memory. Core tests (remind records from the engine copy; a REGRESSION that
no move/flag edit is emitted; wake answers notices + removes rows; no store ->
no-op; conversation refused). Fake-clock timer tests (the snooze_timer_tests
drive() pattern). Notify tests (deliver_reminders honours the enabled switch,
ignores focus, emits the batch; a channel failure never suppresses the OS
notification). Command + web picker/store/event tests. ONE gated live harness
proof (reminder_proof.rs: submit a note, remind_message(note, 1_000), force the
wake with a future now, assert the notice carries the note's subject/sender, the
entry gone, and the note STILL in the INBOX unmoved; withdraw the note). The wake
body takes now, so the proof forces a wake without real sleeping.