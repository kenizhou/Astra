# SP3a plan: the plugin platform's contracts and worker host

Spec: docs/superpowers/specs/2026-10-21-sp3a-plugin-platform-design.md.
Branch `sp3a-plugin-platform` from main. Each task: reds first, the
implementation, the battery, a conventional commit.

### Task 1: the contracts (D1, D2, D6)

**Files:** crates/astra-plugin/src (the envelope family in
`object_model/envelope.rs`; the handle types and their table in
`object_model/refs.rs`; the snapshot structs in
`object_model/snapshots.rs`; the method registry in
`object_model/methods.rs`; the manifest's `methods` field replacing
`hooks`; the widened Permission table; hook_api.rs slimmed to the v1
payload types; protocol.rs and its tests deleted).

- [ ] Tests first; commit `feat: contract the plugin platform's object model`.

### Task 2: the router (D3)

**Files:** crates/astra-plugin/src/router.rs (the three gates:
manifest-declared, permission granted, handle in scope; the typed
refusal enum; proposed-method dev gating).

- [ ] Tests first; commit `feat: route plugin calls through the three gates`.

### Task 3: the worker host and the echo plugin (D4, D5)

**Files:** crates/astra-plugin/src/worker.rs (spawn, the JSONL pump,
the supervision machine with generations, init timeout, restart budget,
fail-closed draining); plugins/echo/ (the conformance binary).

- [ ] Tests first; commit `feat: host worker plugins with sidecar discipline`.

### Task 4: the desktop registry (D7)

**Files:** client/desktop/src/plugin/mod.rs (managed state: discovery,
the grant table, the worker host handles) and the thin command set
(list / enable / disable / approve) registered in lib.rs.

- [ ] Tests first; commit `feat: hold the plugin registry in the desktop host`.

### Task 5: the docs and the record

**Files:** docs/capabilities.md row + zh; the changelog fragment;
docs/plugins.md created (the contract: envelope, handles, permissions,
supervision) + zh.

- [ ] Commit `docs: record the plugin platform slice`.

### Task 6: close-out

- [ ] Full battery; branch pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: contracts (T1), router (T2), worker + echo (T3),
  registry (T4), docs (T5). Every D lands in exactly one task.
- Interface chain: the envelope and handles (T1) -> the router (T2) ->
  the worker speaks the envelope to the echo plugin (T3) -> the host
  manages the whole (T4).
- Risks: the manifest's hook-to-methods switch must keep old manifests
  failing loudly, never silently accepted; the handle table must not
  leak entries across generations (drain on worker death); the
  supervision port must keep the agent supervisor's drop order intact
  (AppState drop order is load bearing); worker.rs stays under the
  500-line cap by keeping the pump and the machine in sibling modules.
