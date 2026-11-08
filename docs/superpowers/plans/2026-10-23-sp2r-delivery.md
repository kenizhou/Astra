# SP2r plan: the delivery batch

Spec: docs/superpowers/specs/2026-10-23-sp2r-delivery-design.md. Branch
`sp2r-delivery` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit.

### Task 1: the core (D2's queue, D3's warm pass, D4's settings, D5's
outage state)

**Files:** crates/astra-app/src (the SyncObserver's committed hook
feeding a notification queue surface with the new/inbox/not-own rules;
the post-pass warm + cap over the engine's warm/drop verbs; the
per-account sync settings on the preferences (depth/cap/cadence,
additive) read by the pass's window and the search depth reporter; the
per-account outage state from pass outcomes); the settings snapshot
rows; the protocol shapes.

- [ ] Tests first; commit `feat: feed the delivery core`.

### Task 2: the host loop (D1, D2's bridge, D5's banner feed)

**Files:** crates/astra-account/src (the watch wrapper over the IMAP
and JMAP concrete watchers, the engine's headroom guard); client/desktop
/src (the per-account driver: sync, watch-or-poll, reconnect with
backoff; the notification bridge over tauri-plugin-notification draining
the core's queue with the focus and summary rules; the offline banner
and outage events to the web).

- [ ] Tests first; commit `feat: run the background delivery loop`.

### Task 3: the surfaces

**Files:** client/web/src (the offline banner; the outage badge on the
account's pane rows; the settings modal's per-account sync section:
depth, cap, cadence, the notifications switch; the search depth link
pointing at the settings).

- [ ] Tests first; commit `feat: surface the delivery batch`.

### Task 4: the live proofs + the docs

**Files:** the gated live suite extensions (an IMAP IDLE Changed for a
delivery made while the watch stands, the loop syncing it and the queue
holding it; a short-cadence poll landing the same way; the warm queue
draining under a cap with a body served from the store); docs/
background-sync.md + zh (created; the rows' cited reference);
docs/sync-progress.md + zh if row 114's citation is this slice's to
resolve (assess); the capabilities rows; the fragment.

- [ ] Commit `feat: prove the delivery batch live and record it`.

### Task 5: close-out

- [ ] Full battery; branch `sp2r-delivery` pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: the queue, warm, settings, and outage state (T1), the
  loop and bridges (T2), the surfaces (T3), live + docs (T4).
- Interface chain: the core state (T1) -> the driver (T2) -> the
  surfaces (T3) -> live + docs (T4).
- Risks: the observer's committed hook must not regress the sync
  progress surface it shares; the watch wrapper must respect the
  engine's headroom (never take the account's last sync connection);
  the notification rules must not fire for the sent filing or
  user-triggered refresh churn; the warm cap must not fight the cache
  (drop only after the warm settles); the per-account settings are the
  first per-account preferences beyond signatures (the shape must
  survive account removal).
