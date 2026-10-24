# Sync progress: the Astra Mail contract

**Scope.** What the app says about a mail sync while it runs: the
per-account progress the status bar shows, and the delivery state (the
offline banner answer and the per-account outage list) that rides the
same snapshot. Binding on every surface that reports sync state.

**Principle.** *One fold, one snapshot.* Every mail pass reports its
lifecycle through the engine's observer seam into one shared fold, the
fold publishes an immutable snapshot only when it moved, and every
surface reads that snapshot. Nothing polls the engine; nothing holds a
second copy of the state.

## The fold

The engine reports a pass's lifecycle per account: started with the
folder count, one folder-finished event per folder whatever the outcome,
then finished. The fold keeps each account's position (how many folders
are done of how many) and its state (syncing or idle), in account order.
A failed folder still counts as finished: the position is what the
surface divides, and a folder that failed is no longer pending.

## The surfaces

- **The status bar lamp.** One clickable lamp per account in the status
  bar: the account's name, a coloured dot, and "syncing" or "up to
  date" from the fold's state; a click asks for a refresh. Before the
  first pull lands nothing else can be true yet, so the bar shows the
  engine's own starting state first, then "No sync running" while the
  fold is empty.
- **The delivery state.** The offline banner answer (true when every
  dialled account is in outage) and the outaged accounts list ride the
  same snapshot, republished the moment the outage fold moves even
  without a pass running (background-sync.md states the rules). The web
  renders the banner as a slim overlay over the shell's top whose words
  name reconnecting, and each outaged account's pane group wears a
  warning badge whose tooltip names the account and its reconnect.

## Known gaps

- **No progress bar under the list**: the position is the status bar's
  lamp today; a started download draws no bar of its own yet.
- **No background-pass status line**: background passes move the same
  lamps, with no separate wording for "a background pass is running".
- **No throttled wording**: a rate-limited pass reports as it is; the
  "Sync paused" wording waits on a surface that distinguishes it.
