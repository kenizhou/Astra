# Background sync: the Astra Mail contract

**Scope.** The delivery of mail while the app runs: the background loop
and its watch-or-poll cadence, the new-mail notification rules, the warm
and cap pass that keeps bodies readable offline, and the outage
classification behind the offline banner and the per-account badges.
Binding on every surface that receives mail. The engine ships every verb
this contract names; the loop owns only the scheduling and the folds.

**Principle.** *A watch is a latency optimisation, never a source of
truth.* Push carries no data: a `Changed` means only "run the account's
normal sync", and the reconciliation is always that sync. A host that
never watches and only polls is fully correct; watching only lowers the
latency of seeing a change.

## The loop

One driver per dialled account in the desktop host, following the
engine's prescribed cycle:

1. **Sync once before trusting the watch.** The driver joins the
   account's first pass (the boot sync or the account-add sync), so the
   watch gap closes before the watch stands and no pass is duplicated.
2. **Watch when the account can.** A `Changed` runs the account's normal
   single-account pass; a `KeepAlive` is a heartbeat that costs nothing;
   an error records its outcome, waits a backoff, and syncs again before
   the next open re-watches.
3. **Poll when it cannot**, at the account's own cadence.

The watch choice is the server's own answer, never a guess: an IMAP
server that advertises no `IDLE` answers poll, a pool whose watch would
take the last connection syncing needs answers poll, and a JMAP session
that advertises no event source answers poll. IMAP `IDLE` watches the
INBOX only, the delivery-critical mailbox, on one dedicated connection
guarded by the engine's watch headroom; every other folder rides the
cadence. JMAP's EventSource watches the account's whole mail face over
the stored password section; an account whose only JMAP credential is an
OAuth grant polls, because the refreshing provider holds its token and
the stream has nothing of its own to present. Graph, Google, and CalDAV
poll.

The cadence is the account's own setting, 15 to 120 minutes, 30 by
default, read fresh each cycle so a change needs no restart. The
reconnect backoff doubles from one second up to the account's cadence
and resets when a watch stands. The loop runs for as long as the process
runs: that is what background delivery means for a desktop app.

Every pass the loop lands dispatches through the app's existing
single-account sync path, so a background pass lands the same snapshots,
outbox drain, warm pass, and surfaces a person-triggered refresh lands.
The outbox drains on every pass, so a send the last run left behind does
not wait for the next one.

## New-mail notifications

Each pass collects the messages that are **new** (the store held no
prior key), land in an **inbox-class** folder, and are **not the
account's own filing** of a message it sent, into a bounded core queue
(the notices oldest first, everything beyond five folded into an
overflow count). The new-versus-changed call is honest by construction:
the queue seeds each account's known keys from the store before its
pass runs, a cold store's first pass is an import rather than a delivery
(history arriving is not new mail), and the suppression clears once a
pass reaches the server.

The host bridge drains the queue after each pass and applies the window
rules: one OS notification per message while the main window is
unfocused (who it is from as the title, what it is about and how it
begins as the body, the preview line present only where the commit's
tier carried one), a single summary notification when more than three
arrived in a pass, none while the window is focused, and none at all
when the General page's switch is off (on by default, read at every
delivery).

## The warm and cap pass

After every pass, the account's in-window mail missing a body is warmed
through the engine's batched warm verb under the account's cap, and the
cap is enforced with the engine's drop-over verb once the warm settles.
Opens are instant for mail that has synced once, and no single message
body exceeds the cap.

The cap is **per message**, not per mailbox: the drop verb forgets
individual oversized sources, and an account's whole offline total is a
later engine surface. The states:

- **A byte figure** (the 256 MiB default): messages whose stated size
  fits warm and stay; larger ones keep their list row and load their
  body on open.
- **Off**: metadata only. Nothing warms, and whatever an open cached is
  dropped on the next pass, so the account's offline copy is its list
  alone.
- **Everything**: any size warms and nothing is ever dropped.

The warm queue is idempotent and resumable by the engine's own contract;
the loop simply drives it after each pass. Attachments warm only when
opened; the cap counts bodies, never attachments.

## The outage rules

The loop's failures are classified per account from the pass outcomes
the engine answers: a streak of **two or more consecutive network-class
failures** puts the account in outage (a warning badge on its pane
rows); **every dialled account in outage** shows the offline banner;
**any successful pass clears** both. A permanent failure is never an
outage: an account whose sign-in died is a dead account the accounts
page reports, and it leaves the dialled set. Store faults never count
either: a broken store that reported itself as an outage would send the
person to check their wifi. A rate-limited server is reachable, so a
rate-limited watch drops its session and backs off without counting.

The offline banner answer and the per-account outage list ride the
sync-progress snapshot (see sync-progress.md), republished the moment
they move, so the banner and the badges always read the latest state. A
manual refresh folds its outcomes through the same state, so a
person-triggered pass clears a standing outage exactly as a background
pass does.

## Known gaps

- **IMAP `IDLE` watches the INBOX only**; other folders ride the cadence.
- **Notifications ride the process running**: closed-to-tray and
  OS-level wake are later platform rows.
- **The cap bounds each message body, not the account's total offline
  footprint**; a total bound needs an engine surface that does not
  exist yet.
- **Narrowing the depth prunes nothing stored**: a depth change narrows
  what syncs from now on (the settings row states the contract).
- **The poll cadence minimum is 15 minutes**; a faster manual refresh is
  the refresh button.
