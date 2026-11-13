# Pinned searches plan

Spec: docs/superpowers/specs/2026-11-13-pinned-searches-design.md. Branch
`pinned-searches` from main at the spec commit. Reds first, implement, battery,
one conventional commit. One contract change: docs/search.md gains the Saved
searches section with its zh mirror in the same commit, the capabilities row
moves with its zh mirror, a changelog fragment lands, and porting-backlog.md
moves pinned searches into the shipped list. Pure client/web; no engine, host,
viewmodel, or ipc.ts change.

### Task: the saved-search store + panel + affordance + contract (D1, D2, D3)

**Files:** a new client/web/src/features/pinned-searches/pinnedSearchesStore.ts
(zustand + localStorage astra.pinnedSearches, cap 20, defensive parse,
logged-not-thrown persist; clone quickStepsStore.ts) + pinnedSearchesStore.test.ts;
a new PinnedSearchesPanel.tsx (list/run/rename/delete + empty state) +
PinnedSearchesPanel.test.tsx; the "Save search" affordance on AmbientIsland.tsx
(captures the live query, absent when blank) + an island test; the panel mount in
TitleBar.tsx (beside the notification-history bell); optional palette commands in
commands.ts; docs/search.md (the Saved searches section) + zh;
docs/capabilities.md (the row) + zh; docs/changelog/unreleased/pinned-searches.md
(Platforms: desktop, Bump: minor); docs/porting-backlog.md (pinned searches into
the shipped list) + zh if mirrored.

- [ ] Confirm at the hook site: Intent::Search / intents.search (ipc.ts:273),
      openSearchIsland (searchIsland.ts:17), the live query echo
      (AmbientIsland.tsx), the quickStepsStore.ts shape, the TitleBar bell mount.
      Reds first (store: parse drops malformed, the cap, CRUD, blank-query
      refusal, idempotent save, persist failure logged; panel: run dispatches
      intents.search with the saved query, rename/delete, empty state; island:
      Save search captures the live query, absent when blank); then the store +
      panel + affordance + docs. Commit `feat: pinned searches`.
- [ ] client/web not cap-exempt: new files in features/pinned-searches/, no
      ipc.ts change (at the cap; the feature owns its types). Running a saved
      search opens the island + dispatches the query at the DEFAULT scope (the
      query carries no scope, search.md rule 7). The query string only is saved.
      vitest traps: wrap mid-test store mutations in act(), mock outer functions
      whole. Comments present tense; no em/en dashes; British English. docs/zh
      mirrors same commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix sidecar
      test; npx tsc --noEmit && npx vitest run (client/web); cargo run -p xtask
      -- gate. If the plugin tests fail with AlreadyGranted, clean the stale
      /tmp/astra-desktop-plugin-* scratch dirs (Windows pid-recycle flake) and
      re-run.
- [ ] The offline battery is the proof (a pure web feature; no live harness
      proof).
- [ ] Push the branch; PR (squash expected); CI watch; squash merge, delete the
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's store + D2's panel/affordance + D3's contract + battery +
  PR (one task).
- Risks: the scope-not-saved rule (search.md rule 7, a pinned search re-runs at
  the default scope); the query-only persistence; the cap + defensive parse;
  ipc.ts at the cap (no change); the vitest act()/mock traps. Device-local, no
  cross-device sync (recorded gap).
- No eval/pipeline bump (model-facing unchanged). The offline battery is the
  proof. Live-proof lesson: clean stale plugin scratch dirs if the battery
  flakes on AlreadyGranted.