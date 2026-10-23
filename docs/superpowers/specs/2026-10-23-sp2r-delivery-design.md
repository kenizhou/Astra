# SP2r design: the delivery batch

**Status.** Decided 2026-10-23. Slices branch from main at this commit.

**Scope.** The delivery batch the backlog names: push and polling (IMAP
IDLE where capable, a polling cadence otherwise), the background loop
that keeps mail arriving while the app runs, new-mail notifications,
offline-first bodies under a per-account cap, per-account fetch depth and
size, and the reconnect/offline banner/outage badge row. The engine ships
every verb this slice wires (the Watch contract, the warm/cap/drop body
machinery, the per-sync window); today nothing in Astra calls any of it,
and refresh happens only when a person asks.

## D1: the background loop

One driver per dialled account in the desktop host, following the
engine's prescribed loop: sync once before trusting the watch, watch when
the account can (a `Changed` runs the account's normal pass; a
`KeepAlive` is a heartbeat; an error reconnects with backoff and syncs
again before re-watching), poll when it cannot. IMAP IDLE watches the
INBOX only (the delivery-critical mailbox; one dedicated connection per
account, guarded by the engine's watch headroom, other folders riding
the cadence); JMAP's EventSource watches the account's whole mail face;
Graph, Google, and CalDAV poll. The cadence is the account's own setting
(15 to 120 minutes, 30 by default). The loop dispatches through the
app's existing single-account sync path, so every pass lands the same
snapshots, outbox drain, and surfaces a person-triggered refresh lands.
The loop runs for as long as the process runs: that is what background
delivery means for a desktop app, and the row says so.

## D2: new-mail notifications

The SyncObserver's `committed` hook, today a deliberate no-op, becomes
the signal: a background pass's `upserted` rows that are new (the store
has no prior key), that land in an inbox-class folder, and that the
account itself did not just send (the sent filing) are collected into a
notification queue the core publishes. The host bridge drains the queue
and emits one OS notification per message (who it is from, what it is
about, how it begins) while the main window is unfocused, one summary
notification when more than three arrived in a pass, and none while the
window is focused (the person is already looking). A settings switch
turns notifications off. The queue is a core surface so the policy is
testable without an OS.

## D3: offline-first bodies

After every pass, the account's in-window mail missing a body is warmed
through the engine's batched warm verb under the account's byte cap, and
the cap is enforced with the engine's drop-over verb, so opens are
instant for mail that has synced once and the store never grows past
the cap. The cap is a per-account setting (a default, an off state for
metadata-only accounts, and an everything state). The warm queue is
idempotent and resumable by the engine's own contract; the loop simply
drives it after each pass.

## D4: per-account fetch depth and size

The per-account sync settings land on the preferences (additive): the
depth in months (the seam the sync pass's window comment names; the
search depth reporter reads the same value so list and search agree),
the body cap of D3, and the cadence of D1. The depth rides the per-sync
window (changeable without reconnecting); narrowing a stored account
prunes nothing silently (the engine's prune verb stays unwired this
slice; a depth change narrows what syncs from now on, and the row's
"every folder synced within the depth" is the contract). The settings
modal gains the per-account section the settings row names as absent.

## D5: reconnect, the offline banner, the outage badge

The loop's failures are classified per account: a retryable
network-class failure streak of two or more marks the account in
outage (a badge on its pane rows); every dialled account in outage from
network-class errors shows the offline banner; any account's next
successful pass clears its badge and the banner. A permanent failure is
never an outage (it is a dead account the accounts page already
reports). The state is a core surface driven by the loop's pass
outcomes, so the banner and badge are testable and the host stays thin.

## Known gaps recorded, not hidden

- IMAP IDLE watches the INBOX only; other folders ride the cadence.
- Notifications ride the process running; closed-to-tray and OS-level
  wake are later platform rows.
- The warm cap counts bodies, not attachments; a message's attachments
  warm only when opened (the engine's attachment reads stay lazy).
- Narrowing the depth does not prune what an earlier depth already
  stored (the prune verb stays unwired).
- The poll cadence minimum is 15 minutes; a faster manual refresh is
  the refresh button.

## Test-first

Every behaviour lands with its red: the notification queue's rules over
fakes (new only, inbox-class only, not own sends, the three-then-summary
bound, the focused suppression at the host seam), the warm and cap pass
against the engine's in-memory store, the per-account settings' round
trip and their effect on the pass window and search depth, the outage
classification, and the live proofs: an IMAP IDLE `Changed` arriving for
a mail delivered while the watch stands, the loop syncing it and the
queue holding it; a short-cadence poll pass landing the same way; the
warm queue draining under a cap with opens served from the store.
