# Junk blocking: the Astra Mail contract

**Scope.** Silencing a sender: block a sender so their mail no longer raises a
new-mail notification. Binding on every surface that ships the verb. It is
NOTIFICATION SUPPRESSION ONLY, the locked kylins reference's red line: the
blocklist drives the new-mail suppression and nothing else. Blocked senders'
mail still lands in the Inbox. The blocklist is a preferences field; the
suppression is a deterministic leg in the existing new-mail queue gate, not the
AI incoming pass, so no eval or pipeline bump. No engine change, no new store.
The gaps are recorded at the end, never left silent.

**Principle.** *A block silences, it never files.* Filing mail as junk stays the
Junk verb's job. The blocklist moves no message and changes no flag; a blocked
sender's mail arrives in the Inbox exactly as before, only quietly. The optional
report leg below reuses the shipped spam report, whose filing is its own act,
not the block's.

## The blocklist preference

A `blocked_senders: BTreeSet<String>` field on `Preferences` (the `sender_names`
pattern, `skip_serializing_if` empty), holding normalised bare addresses. An
address is normalised lowercased, trimmed, and lifted out of a display form
("Ada <ada@x.com>") to its addr-spec, the VIP set's `normaliseAddress` idiom
mirrored core-side, so every spelling of one sender agrees on one key. A blank
address stores nothing.

`BlockSender` and `UnblockSender` are `SettingsIntent` arms riding the shared
`set_setting` setter: the address crosses verbatim and the core normalises it on
dispatch, the zone id's own discipline. The block answers on the settings block
(`blocked_senders`), so the surface reads it through the existing settings wire;
no new IPC type. An empty blocklist writes no entry, the expansion maps' own
canonicalisation. The blocklist is device-local (the preferences file); it does
not roam.

## The suppression gate

The new-mail queue refreshes the blocked set at arm, the way the pass already
reads the sync settings, and `qualifies` gains a "sender not blocked" leg beside
the inbox-class and own-filing legs. A blocked sender therefore raises neither
the OS notification nor the history row: the host bridge drains the queue, so a
row the gate never holds reaches neither channel.

Two invariants the gate keeps:

- **Fail-open on the filter.** A failed blocklist read never suppresses a
  notification. The preferences load fails open to defaults (an empty blocklist),
  and an empty set suppresses nothing, so an unreadable file silences no mail.
- **Blocked beats VIP.** The block leg is an absolute veto with no bypass, the
  reference order. Astra's VIP is web-local view state and owns no notification
  bypass in this lane, so the veto needs no VIP carve-out; the web suite pins the
  mark-and-block ordering where the VIP set lives.

The gate is the deterministic new-mail lane, not the AI incoming pass: the
suppression is a preference read, never a model judgement, so no eval run or
pipeline version bump rides it.

## The surface

The row menu's "Block sender…" entry (a single row; a batch stands disabled with
its reason) opens a standalone `BlockSenderDialog` over the row's sender. The
dialog blocks the address and offers an optional "Also report this conversation
as junk" checkbox, defaulted on, shown only where the account can report (the
spam verb's own visibility rule: the provider can report and the tree resolves a
junk destination). The report reuses the shipped `mark_junk` verb. The block is
the durable act and stands whatever the report does: a report failure is logged,
never undoing the block. Cancel and Escape dispatch nothing.

The settings management section lists the blocked senders with an Unblock each,
reading the settings wire. It lands in the General category beside the new-mail
notifications switch: Astra has no "Mail" settings category, and the blocklist
governs how arriving mail announces itself, an app-level behaviour. The section
says plainly that blocked mail still arrives in the Inbox.

## Known gaps

- **Suppression only.** Blocked senders' mail still lands in the Inbox;
  auto-filing to Junk is a named non-goal (the reference red line; filing stays
  the Junk verb's job). Auto-filing would be a separate, more invasive slice.
- **Device-local.** The blocklist is the preferences file; it does not roam
  across devices, and the CLI does not share it (the snooze/reminders precedent).
- **No CLI verb.** The blocklist is a preference the surfaces read and write (the
  VIP/pin precedent, pure view state over a preference); no new CLI verb ships.
- **The settings category.** The management section lands in General, an existing
  category, because Astra has no "Mail" category; a surface detail, recorded.