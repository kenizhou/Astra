# SP3b plan: the send gate and the compose transform

Spec: docs/superpowers/specs/2026-10-21-sp3b-send-gate-design.md. Branch
`sp3b-send-gate` from `sp3a-plugin-platform` until PR #31 merges. Each task:
reds first, the implementation, the battery, a conventional commit.

### Task 1: the gate contracts (D1, D3)

**Files:** crates/astra-plugin (the manifest's `before_send_mode`, required
when `before_send` is declared; the `SendMode` wire type); plugins/echo (the
`before_send` allow answer and the transform echo); crates/astra-app's new
`gate.rs` (the `BeforeSendGate` and `ComposeTransform` ports, the
`GateComposition` and `GateVerdict` types); crates/astra-viewmodel's
sending.rs (the `HeldSend` row and the `SendingSnapshot.held` leg, wire
pinned).

- [ ] Tests first; commit `feat: contract the send gate and its held rows`.

### Task 2: the submit seam (D2, D4)

**Files:** crates/astra-app (the gate and transform calls in submit.rs's
arms ahead of `submit_mail`; the held-send bookkeeping in outbox.rs; the
prompt answer and held retry/withdraw intents; the snapshot publishing).

- [ ] Tests first; commit `feat: gate every send ahead of the queue`.

### Task 3: the desktop gate (D5)

**Files:** client/desktop/src/plugin/gate.rs (the two ports over the
registry's workers, the per-plugin timeout and mode folding, the admin-grant
check); the boot installs the gate when a serving plugin is activated; the
prompt answer rides the existing dispatch path (the `AnswerGate` intent, no
new command); the echo-over-duplex integration test.

- [ ] Tests first; commit `feat: fold the plugin workers into the send gate`.

### Task 4: the live proof + the docs (D6)

**Files:** crates/astra-app/tests_live_harness (the scripted veto gate: a
blocked astra1 send never reaches astra2; lifted, it lands); docs/plugins.md
+ zh (the send gate section); docs/capabilities.md rows + zh; the changelog
fragment.

- [ ] Commit `feat: prove the send gate live and record it`.

### Task 5: close-out

- [ ] Full battery; branch pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: ports and manifest (T1), the seam (T2), the host (T3),
  live + docs (T4). Every D lands in exactly one task.
- Interface chain: the contract types (T1) -> the app seam consumes them
  (T2) -> the desktop gate implements them over real workers (T3) -> live
  proof (T4).
- Risks: the gate must run after the draft builds (the draft is what the
  transform rewrites) but before queueing (never after); a prompt answer
  must re-enter the same submit tail, not a parallel one; the held row must
  never survive its draft being discarded; the echo integration must not
  leak a spawned process (the duplex spawn is in-process by construction);
  the live veto must fail closed on the harness (a gate error blocks, never
  passes).
