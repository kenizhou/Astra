# Snooze design

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** There is no way to set a message aside and have it return later. This
kylins addition adds snooze: pick a wake time, the message moves to a Snooze
folder, and at the wake time it returns to its origin folder. The engine has no
snooze/scheduled mechanism, so this is a host/core feature: a device-local snooze
store, a host wake timer, and two moves. No engine change; no eval/pipeline bump.

Two facts dictate the design (verified against the engine):
- An IMAP move synthesizes a NEW provider key in the destination folder
  (mail_edit.rs MailEditReceipt: the destination copy reconciles on the next
  sync). The move-back therefore RE-RESOLVES the message by its Message-ID
  header after a sync; it never reuses the key stored at snooze time.
- There is no Snooze MailboxRole (role.rs). Snooze creates a "Snooze" folder per
  account (FolderIntent::Create) and moves into it.

## D1: the snooze store

A new crate crates/astra-snooze (copying astra-ai's AiStore: rusqlite under a
Mutex, embedded migrations, open with the 5s busy timeout, open_in_memory for
tests). One row per snoozed message: id, account_id, message_id (the RFC 5322
Message-ID header, the STABLE identity), provider_key (the key at snooze time,
diagnostic only, goes stale on move), wake_at (unix seconds), origin_folder (the
folder key to return to), subject, sender, created_at; an index on wake_at. The
identity is newtyped across the boundary (AccountId / MessageIdHeader). Methods:
record, due(now) (wake_at <= now), remove, list (for a future Snoozed view). The
store is device-local; the CLI does not share it (a host-timer feature).

## D2: the core snooze + wake

A core install_snooze_store (the install_analytics_store shape, a
Mutex<Option<Arc<Store>>> slot; no AppState field, the drop-order trap) and two
core methods:

- snooze_message(account, row, wake_at): ensure the account's Snooze folder
  exists (create if absent, resolve its key), read the message's Message-ID +
  origin folder, record the entry, then dispatch MoveMessages into the Snooze
  folder (the mail_writes move path). A conversation row is refused (moves name
  messages only).
- wake_due_snoozes(now): for each due entry, sync the Snooze folder (so the moved
  copy reconciles), RE-RESOLVE the message by Message-ID (scan engine.messages
  filtered to the Snooze folder; subject/sender as fallback), dispatch
  MoveMessages back to origin_folder (or Inbox when the origin is gone), remove
  the entry. The move republishes Surface::MailboxList, so the web learns of the
  wake through the existing surface_changed loop (no new event).

## D3: the host timer + the surface + the contract

A host snooze_timer.rs copying calendar_timer.rs: a SnoozeTimer<H: SnoozeHost>
state machine (wait/due/wake seams), a ~30-60s tick, the debug interval/limit
overrides (ASTRA_SNOOZE_*), spawned in setup_host beside calendar_timer::spawn
holding Arc<dyn AppPort> + Arc<SnoozeStore> (the drop-order-safe shape). Each tick
reads due entries from the store and calls wake_due_snoozes(now) through AppPort;
the timer is stateless over the store, so a restart re-reads due entries (a wake
due while the app was closed fires at next launch).

The surface: a row-menu "Snooze…" entry (single message, the sweepItem shape)
opening a new features/snooze/ picker (snoozeStore.ts + SnoozeDialog.tsx, the
sweep dialog pattern, useDialogFocus) offering later today / tomorrow / next week
as computed instants. The intent constructor + wire types ride a new
services/snooze.ts (never ipc.ts, at the cap); new desktop commands in a
commands/snooze.rs submodule (commands.rs is at the cap), registered in lib.rs.

The contract: a new docs/snooze.md (the store, the Message-ID re-resolution, the
Snooze folder, the wake timer, the closed-app wake precision) + zh; a
docs/list-selection.md cross-reference; the capabilities row; a changelog
fragment. The zh mirrors move in the same commit.

## Known gaps recorded, not hidden

- Device-local: the snooze store does not roam across devices.
- Wake precision while the app is closed: the timer runs only while the process
  lives; a wake due during a closed app fires at next launch, not the exact
  instant (inherent to a desktop host timer).
- Custom wake time is a fast-follow (the native datetime-local reuse exists).
- A dedicated Snoozed view/filter is a follow-up (the Snooze folder is reachable
  via the pane).
- Conversation snooze, re-snooze/undo, and a CLI/agent snooze verb are follow-ups.

## Test-first

Every behaviour lands with its red. Store tests (record/due/remove round-trip,
the due query boundary, migration claim, malformed-row tolerance) over
open_in_memory. Timer tests with a fake clock (the calendar_timer_tests pattern:
a fake SnoozeHost records Wait/Wake, scripts now/due, drives step(); no real
sleeping). Core integration tests (snooze records + moves out; wake re-resolves
by Message-ID and moves back + removes the entry; a key-reuse regression test
pins that the stored key is NOT used for the move-back). ONE gated live harness
proof (snooze_proof.rs: create the Snooze folder, snooze a seeded note, force a
wake with a future now, refresh, assert the note is back in INBOX and the entry
gone, withdraw the Snooze folder). The wake body takes now as a parameter, so the
proof forces a wake without real waiting.