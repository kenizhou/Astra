# Junk blocking design

**Status.** Decided 2026-11-14. Slices branch from main at this commit.

**Scope.** There is no way to silence a sender. This kylins addition adds junk
blocking: block a sender so their mail no longer raises a new-mail notification.
It is NOTIFICATION SUPPRESSION ONLY, the locked kylins reference's red line: the
blocklist drives the new-mail suppression and nothing else; filing mail as junk
stays the Junk verb's job (the optional report leg reuses the shipped spam report).
Blocked senders' mail still lands in the Inbox. The blocklist is a preferences
field; the suppression is a deterministic leg in the existing new-mail queue gate
(not the AI incoming pass, so no eval/pipeline bump). No engine change; no new
store; no eval/pipeline bump.

## D1: the blocklist preference + the suppression gate

A blocked_senders: BTreeSet<String> field on Preferences (the sender_names /
signatures pattern, skip_serializing_if empty), addresses normalised lowercased /
trimmed / addr-spec lifted from display form (the VIP normaliseAddress idiom). A
BlockSender / UnblockSender SettingsIntent arm riding the shared set_setting
setter. The new-mail queue refreshes the blocked set at arm (the way the pass
already reads sync settings) and qualifies gains a "sender not blocked" leg, so a
blocked sender raises neither the OS notification nor the history row (the kylins
"blocked ids consumed before history" semantics). A failed blocklist read never
suppresses a notification (fail-open on the filter, the kylins invariant). A
blocked sender beats even a VIP mute-bypass (the reference order).

## D2: the web block surface

A row-menu "Block sender" item (thin, beside vipItem; rowMenu.ts near the cap, so
the dialog is its own component). A standalone BlockSenderDialog: blocks the
address, plus an optional "Also report this conversation as junk" checkbox
defaulted on, reusing the shipped spam report verb (a report failure never undoes
the block). A settings management section listing blocked senders with unblock
(Astra has no "Mail" settings category; the section lands in an existing category,
a recorded surface detail). The blocklist is device-local (the preferences file);
the web reads it through the settings wire (no new ipc.ts type, the field rides the
existing settings snapshot).

## D3: the contract

A new docs/junk-blocking.md (the suppression-only red line, the blocklist
preference, the gate leg, the optional report leg, the fail-open invariant, the
recorded gaps) + zh; the capabilities row; a changelog fragment; the
porting-backlog shipped-list move. The zh mirrors move in the same commit.

## Known gaps recorded, not hidden

- Suppression only: blocked senders' mail still lands in the Inbox; auto-filing to
  Junk is a named non-goal (the reference red line; filing stays the Junk verb's
  job). If the owner wants auto-filing, that is a separate, more invasive slice.
- The blocklist is device-local (the preferences file), does not roam; the CLI does
  not share it (the snooze/reminders precedent); no new CLI verb (VIP/pin precedent,
  pure view state over a preference).
- The settings management section lands in an existing category (Astra has no
  "Mail" category), a surface detail.

## Test-first

Every behaviour lands with its red. Core tests (the settings intent round-trip,
the address normalisation, the queue classification suppresses a blocked sender's
notice, an unblocked sender still qualifies, a failed blocklist read fails open,
the blocked-beats-VIP order). Web tests (the block dialog blocks + the optional
report leg, a report failure never undoes the block, the management section
unblocks). ONE gated live harness proof (junk_blocking_proof.rs: block a sender,
deliver a message from them through the harness submission path, assert the queue
holds NO notice; unblock, deliver again, assert the notice returns; withdraw
copies). No live model proof; the offline battery is the proof.