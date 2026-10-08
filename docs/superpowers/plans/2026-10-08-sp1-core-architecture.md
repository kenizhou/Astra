# SP1 Core Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Astra core architecture (engine-pinned workspace, Intent/Surface protocol v1, Tauri host ports, minimal React shell, plugin skeleton, Stalwart harness) and prove it with the vertical slice: add an IMAP account, sync, see the folder tree, survive a restart.

**Architecture:** Port mail-calendar's core crates (`mailcal-app`/`viewmodel`/`account` patterns) into `astra-*` crates consumed directly by a Tauri host that replaces `mailcal-bindings` (runtime owner, keyring credential store, debounced observer bridge, `dispatch`/pull commands). The React SPA is a thin renderer pulled by `surface_changed` events. Sync rounds, snapshots, and the durable outbox discipline live in `astra-app`.

**Tech Stack:** Rust (edition 2021, workspace lints mirroring mail-calendar), the nine engine crates git-pinned at rev `68eab00e`, Tauri 2 + keyring for the host, React 19 + Vite 7 + Tailwind v4 + Zustand for the SPA, Docker Stalwart for the live harness.

**Spec:** `docs/superpowers/specs/2026-10-08-sp1-core-architecture-design.md`

## Global Constraints

- Every `.rs`, `.ts`, `.tsx` file starts with `// SPDX-License-Identifier: GPL-3.0-only` (gate-enforced).
- Source files stay under 500 lines; split protocol families the way mail-calendar splits them.
- Intents and Surfaces are serde-serialisable: externally tagged enums, snake_case, `None` omitted on the wire.
- The engine is consumed only through `engine-api` (provider crates allowed inside `astra-account`), pinned at `rev 68eab00e6a9a9fe5a200218e156dbaae1ba00caa` in `[workspace.dependencies]`.
- No product SQLite in SP1: engine SQLite plus `preferences.toml` plus the OS keyring (spec D4).
- Every product operation flows through `dispatch`; no Tauri command mutates engine or product state directly.
- Tests are inline sibling files (`<module>_tests.rs`); app-level tests use `Engine::open_in_memory`.
- British English prose; no dash as punctuation; English only outside `docs/zh/`.
- `cargo run -p xtask -- gate` green before every commit; conventional commits.
- Port source root: `D:/Projects/mailclient/opensource/mail-calendar` (abbreviated MC below). kylins root: `D:/Projects/mailclient/kylins` (abbreviated KY).

## Adaptation rules (apply to every port task)

- **AR1**: rename `mailcal` -> `astra` in crate names, module paths, identifiers, doc comments; drop history comments (nothing about ports, phases, or upstream in comments).
- **AR2**: remove all `uniffi` attributes and the `mailcal-bindings` conversion layer; add `serde::Serialize`/`Deserialize` derives where values cross the Tauri IPC (Intent, Surface, snapshots, status enums, setup types). Wire style: default externally tagged, `#[serde(skip_serializing_if = "Option::is_none")]` on optional fields.
- **AR3**: engine types (ids, rows, drafts) are imported from `engine-api` re-exports and never redefined.
- **AR4**: dates use the `time` crate, matching the engine.
- **AR5**: mirror MC's `[workspace.lints]` block from its root `Cargo.toml` verbatim (unsafe forbid, warnings/missing_docs/unreachable_pub deny, clippy pedantic deny with its allows); fix ported code until clean.
- **AR6**: workspace `[workspace.dependencies]` entries carry the versions listed in the MC table minus uniffi, plus `tauri = "2"`, `tauri-plugin-single-instance`, `tauri-plugin-opener`, `tauri-plugin-dialog`, `tauri-plugin-notification`, `keyring` (latest 3.x) for the host only.

---

### Task 1: Engine wiring and workspace lints

**Files:**
- Modify: `Cargo.toml` (members, workspace deps, lints)
- Create: `.cargo/config.toml`

**Interfaces:**
- Produces: nine git-pinned engine deps under `[workspace.dependencies]` (`engine-api`, `engine-core`, `engine-provider`, `engine-tls` with `tls-native-erts`, `provider-imap`, `provider-caldav`, `provider-graph`, `provider-google`, `provider-jmap`); the MC lint block; shared `target-dir`.

- [ ] **Step 1:** Add the engine git dependencies to root `Cargo.toml` copying the exact `rev`, features, and per-crate spelling from `MC/Cargo.toml` `[workspace.dependencies]`.
- [ ] **Step 2:** Copy MC's `[workspace.lints]` block per AR5 and inherit it from every member via `[lints] workspace = true`.
- [ ] **Step 3:** Create `.cargo/config.toml` with a shared build dir comment header and, as comments, the `[patch."https://github.com/allodia-eu/email-calendar-sync-engine"]` local-checkout pattern from MC's `.cargo/config.toml`.
- [ ] **Step 4:** Create a throwaway `crates/astra-viewmodel` stub (empty lib.rs with SPDX header) as the first member so the pins actually resolve, run `cargo update -p engine-api 2>/dev/null; cargo build -p engine-api`, expect success, then `cargo tree -p engine-api --depth 1 | head -20` recorded in the report.
- [ ] **Step 5:** `cargo run -p xtask -- gate` green; commit `build: pin the engine crates and adopt the mail-calendar lint set`.

### Task 2: astra-viewmodel (snapshot types)

**Files:**
- Create: `crates/astra-viewmodel/{Cargo.toml, src/lib.rs, src/view.rs, src/folders.rs, src/view_tests.rs, src/folders_tests.rs}`
- Port from: `MC/crates/mailcal-viewmodel/src/{view.rs, folders.rs}` restricted to the MailboxList surface.

**Interfaces:**
- Consumes: `engine-api` (id newtypes, `MailboxListRow` sources).
- Produces: `MailboxListSnapshot { account_folders: Vec<AccountFolderRow> }`, `AccountFolderRow` (account id, display name, folders with nesting, unread counts, role, expanded), serde derives per AR2.

- [ ] **Step 1:** Write `view_tests.rs` first: a snapshot round trips through `serde_json` with the exact wire shape asserted (external tagging, snake_case, `None` omitted).
- [ ] **Step 2:** Port `view.rs`/`folders.rs` per AR1-AR4 keeping only what `MailboxListSnapshot` needs; drop calendar/contacts/settings modules entirely.
- [ ] **Step 3:** `cargo test -p astra-viewmodel` green; clippy pedantic clean; gate green; commit `feat: port the mailbox list view model from mail-calendar`.

### Task 3: astra-account config and setup

**Files:**
- Create: `crates/astra-account/{Cargo.toml, src/lib.rs, src/config.rs, src/config_tests.rs, src/setup.rs, src/setup_tests.rs, src/credentials.rs, src/tls.rs}`
- Port from: `MC/crates/mailcal-account/src/{config.rs, setup.rs, imap_credentials.rs, tls.rs}`.

**Interfaces:**
- Produces: `AccountConfig` (TOML `[imap]`/`[smtp]` sections, `Secret` redacting Debug, `load`/`load_str`/`to_toml`, `derived_account_id`, pinned `shape.id`), `SetupCredential::{Password, Grant}`, `build_config_toml` with default port rules, `imap_credential_source` over an injected `CredentialSource`, TLS acceptor with certificate exception pinning types.

- [ ] **Step 1:** Write `config_tests.rs` first: TOML round trip, secret redaction in Debug, derived id stability (username case), blank-username rejection.
- [ ] **Step 2:** Write `setup_tests.rs` first: bare host gets standard ports, CalDAV host gains `https://`, password credentials serialise into the TOML.
- [ ] **Step 3:** Port the four modules per the adaptation rules; `to_toml` remains the only serialisation point.
- [ ] **Step 4:** Tests, clippy, gate; commit `feat: port account config, setup, credentials, and tls from mail-calendar`.

### Task 4: astra-account IMAP connections

**Files:**
- Create: `crates/astra-account/src/{imap.rs, imap_tests.rs, reconnect.rs, pass_folders.rs}`
- Port from: `MC/crates/mailcal-account/src/{imap.rs, reconnect.rs, pass_folders.rs}`.

**Interfaces:**
- Produces: `ImapConnections` (one `ImapAccount<TlsStream<TcpStream>>` per registered account, async-mutex dial dedup), `connect_mail_providers` (connect, LIST, bind folders that `pass_syncs`, INBOX first), `connect_imap_mailbox`, `ReconnectingImapProvider` (single retry through an injected `Redial` on `FailureClass::Retryable`).

- [ ] **Step 1:** Write `imap_tests.rs`/`reconnect` tests with a fake `Provider` (pattern from `MC/crates/mailcal-account/src/imap.rs` tests and `tests_fakes`): bind order puts INBOX first; retryable failure triggers exactly one redial; non-retryable propagates.
- [ ] **Step 2:** Port per the rules; the fake provider lives under `src/fakes.rs` if needed to stay under 500 lines.
- [ ] **Step 3:** Tests, clippy, gate; commit `feat: port imap connections, folder binding, and reconnect wrapper`.

### Task 5: astra-app protocol v1

**Files:**
- Create: `crates/astra-app/{Cargo.toml, src/lib.rs, src/protocol.rs, src/protocol/{intent.rs, status.rs}, src/reference.rs, src/surfaced.rs, src/protocol_tests.rs, src/surfaced_tests.rs, src/reference_tests.rs}`
- Port from: `MC/crates/mailcal-app/src/{protocol.rs, protocol/intent.rs (subset), protocol/status.rs (subset), reference.rs (subset), surfaced.rs}`.

**Interfaces:**
- Produces: `Intent` with exactly `RefreshMail`, `SelectAccount(Option<AccountId>)`, `SetAccountExpanded{account, expanded}`, `SetFolderExpanded{folder: FolderRef, expanded}`, `SelectFolder{folder: FolderRef}`, `SetViewMode(ViewMode)`, `ShowMore`, `Folders(FolderIntent)` (only `Create` declared, `#[non_exhaustive]`); `Surface::{MailboxList, SyncProgress}`; `FolderRef::from_parts(&str, String) -> Option<Self>`; `Surfaced<T>` with publish-then-signal and clone-only `get()`; serde on all of it per AR2.

- [ ] **Step 1:** Write `protocol_tests.rs` first: every variant round trips through JSON with the exact wire names asserted; `FolderRef::from_parts` rejects malformed halves.
- [ ] **Step 2:** Write `surfaced_tests.rs` first: publish signals the observer exactly once after the value is visible; `get()` never signals; `resignal` is the only re-signal path.
- [ ] **Step 3:** Port the files; drop every Intent/Surface/status variant not in the interface list (they return with their SP2 slices).
- [ ] **Step 4:** Tests, clippy, gate; commit `feat: port intent/surface protocol v1 with serde wire shapes`.

### Task 6: astra-app dispatch, snapshot, accounts, sync, boot

**Files:**
- Create: `crates/astra-app/src/{dispatch.rs, dispatch_tests.rs, snapshot.rs, window.rs, row_cache.rs, accounts.rs, sync.rs, sync_account.rs, preferences.rs, preferences_tests.rs, boot.rs}`
- Port from: `MC/crates/mailcal-app/src/{dispatch.rs, snapshot.rs, window.rs, row_cache.rs, accounts.rs, sync.rs, sync_account.rs}` plus `MC/crates/mailcal-account/src/preferences.rs` and the boot sequence from `MC/crates/mailcal-bindings/src/boot.rs`.

**Interfaces:**
- Produces: `App<P: Provider>::dispatch(&self, intent: Intent)` (async, one arm per variant, `Folders` arm returns a not-yet error); `App::new(engine: Arc<Engine>, accounts, connector, observer, clock)`; `rebuild_snapshot` with search-generation guard and windowed `visible_limit` + `ShowMore`; `add_new_account_deferred` (install, rebuild, default 3-month depth, no sync) and `sync_added_account`; `sync_mail` fan-out via `StreamTuning::new(200, 50)`; `preferences.toml` (view mode, expansion, selected folder) round trip; `boot()` sequence: open engine, `abandon_sync_leases`, construct app, `recover_outbox`, `prime_snapshot`, spawn connect + sync.

- [ ] **Step 1:** Write `dispatch_tests.rs` first with `Engine::open_in_memory` and the fake provider: `SelectFolder` persists to preferences and rebuilds `MailboxList`; `RefreshMail` fans out sync and publishes; `ShowMore` grows the window.
- [ ] **Step 2:** Write `preferences_tests.rs` first: defaults, round trip, expansion map updates.
- [ ] **Step 3:** Port the modules; the observer is the `AppObserver` trait ported from MC `protocol.rs` with `Send + Sync` bounds; telemetry hooks are dropped (no telemetry in SP1).
- [ ] **Step 4:** Tests, clippy, gate; commit `feat: port dispatch loop, snapshots, sync, preferences, and boot`.

### Task 7: astra-plugin skeleton

**Files:**
- Create: `crates/astra-plugin/{Cargo.toml, src/lib.rs, src/manifest.rs, src/manifest_tests.rs, src/lifecycle.rs, src/hook_api.rs, src/permissions.rs}`

**Interfaces:**
- Produces: `PluginManifest` (id semver, name, versioned hook API level, declared permissions) parsed from TOML with redacting Debug for secrets-free manifests; `Lifecycle::{Discovered, Loaded, Activated, Failed(String)}` state machine transitions; `HookApiV1::{ComposeTransform, MessageDecode}` as typed request/response contracts (serde); `Permission::{ReadMailbox, TransformOutgoing, DecodeIncoming}` with the fail-closed default table (everything needs approval). No process execution yet (spec D6).

- [ ] **Step 1:** Write `manifest_tests.rs` first: valid TOML parses; API level mismatch and missing permission declarations are rejected; unknown fields are errors.
- [ ] **Step 2:** Write lifecycle transition tests (illegal transitions rejected).
- [ ] **Step 3:** Implement the types; unit-test the permission table's fail-closed default.
- [ ] **Step 4:** Tests, clippy, gate; commit `feat: add plugin host skeleton (manifest, lifecycle, hook api v1 types)`.

### Task 8: Stalwart harness

**Files:**
- Create: `docker/harness/{compose.yaml, stalwart/config.toml, stalwart/seed/*.mjs or .sh}`, `scripts/dev/harness.sh`
- Port from: `MC/docker/**` and `MC/scripts/dev/harness.sh` (read MC's harness for the seeding shape; Astra needs two seeded IMAP/SMTP accounts with distinct folders).

**Interfaces:**
- Produces: `scripts/dev/harness.sh up|down|reset` starting a seeded Stalwart on `127.0.0.1:2143` (IMAP) and `2025` (SMTP); env `ASTRA_LIVE_HARNESS=1` plus `ASTRA_HARNESS_IMAP_HOST/PORT/USER/PASSWORD` exported by the script; a `tests_live_harness.rs` gate-gated test in `astra-app` connecting via Task 4's `ImapConnections` and asserting the folder list.

- [ ] **Step 1:** Read MC's docker harness and port the minimal compose + config; seed two users (`astra1@example.test`, `astra2@example.test`) with password `astra-harness`, one custom folder each.
- [ ] **Step 2:** Write `harness.sh` with `up|down|reset` and a `env` subcommand printing the exports.
- [ ] **Step 3:** `bash scripts/dev/harness.sh up` then `docker ps` shows the container healthy; IMAP LOGIN verified with a one-shot script (e.g. via the test).
- [ ] **Step 4:** Write the gated test (skips unless `ASTRA_LIVE_HARNESS=1`), run it green against the harness; gate stays green when the harness is down; commit `test: port the stalwart harness and add the gated live folder test`.

### Task 9: Tauri host scaffold and runtime

**Files:**
- Create: `client/desktop/{Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, src/main.rs, src/lib.rs, src/runtime.rs, src/runtime_tests.rs}`
- Port from: `KY/kylins.client.backend/{tauri.conf.json, capabilities/default.json, src/shell/desktop.rs (builder shape)}` and `MC/crates/mailcal-bindings/src/runtime.rs`.

**Interfaces:**
- Produces: the frameless main window (1200x800, min 800x600, `com.kenizhou.astra`, icons from `client/desktop/icons/`); plugin order single-instance, deep-link (mailto reserved), opener, dialog, notification; `build_runtime()` (workers = cores minus one) plus the large-stack blocking-call helper ported from MC's `FfiRuntime::block_on`; managed state registered in setup.

- [ ] **Step 1:** Scaffold the crate; `tauri.conf.json` with `frontendDist` pointing at `../web/dist` (placeholder allowed until Task 12 exists; use `../web/dist` and create an empty `client/web/dist/.gitkeep` so the build resolves, removed in Task 12).
- [ ] **Step 2:** Port the runtime module; test: `build_runtime()` returns a runtime whose worker count matches the formula on this machine.
- [ ] **Step 3:** `cargo build -p astra-desktop` succeeds on Windows; add `client/desktop` to workspace members but EXCLUDE it from `default-members` (so `cargo clippy --workspace` on CI ubuntu without webkit deps stays green).
- [ ] **Step 4:** Gate green; commit `feat: scaffold the tauri host with runtime and window`.

### Task 10: Tauri host ports and commands

**Files:**
- Create: `client/desktop/src/{app_state.rs, credential_store.rs, observer.rs, observer_tests.rs, commands.rs, commands_tests.rs, boot.rs, exit.rs}`
- Port from: `MC/crates/mailcal-bindings/src/{credential_store.rs (trait shape), observer.rs (DebouncedObserver), app_accounts.rs (add_account flow), boot.rs (sequence), token-free}`; keyring implementation new.

**Interfaces:**
- Produces: `AccountCredentialStore` implemented over the `keyring` crate (service `astra`, account-scoped entries, rollback on persist failure); `dispatch(intent: Intent)` command (serde from the webview, spawns on the runtime); `mailbox_list() -> MailboxListSnapshot` and `sync_progress()` pull commands; `surface_changed` event emission through a `DebouncedObserver` (250 ms, leading for MailboxList, trailing for SyncProgress); `add_account(config_toml)` (prepare, dial, persist, `add_new_account_deferred`, spawn `sync_added_account`); `imap_test_connection`; exit hook stopping sync with a five second bound.

- [ ] **Step 1:** Write `commands_tests.rs` first with a fake `App` behind the same trait seam: `dispatch` accepts the JSON of every Intent variant; `mailbox_list` returns the published snapshot; unparsable intent yields a typed error, never a panic.
- [ ] **Step 2:** Write `observer_tests.rs` first: N publishes inside the window coalesce to one leading-edge MailboxList signal; SyncProgress signals trail.
- [ ] **Step 3:** Implement; the managed state is one `AppState { app: Arc<App<..>>, runtime, credential_store }`.
- [ ] **Step 4:** Tests, clippy (pedantic allowed to scope down in the tauri crate only with a recorded ruling), gate; commit `feat: add host ports, ipc commands, and boot wiring`.

### Task 11: React SPA scaffold

**Files:**
- Create: `client/web/{package.json, vite.config.ts, tsconfig.json, index.html, src/main.tsx, src/App.tsx, src/styles/{globals.css, theme.css}, src/vite-env.d.ts}`
- Port from: `KY/kylins.client.frontend/{package.json (dependency versions), vite.config.ts, src/styles/theme.css (token set)}`.

**Interfaces:**
- Produces: Vite 7 + React 19 + TS strict + Tailwind v4 via `@tailwindcss/vite`, port 5173 strict, `@` alias; theme tokens as CSS custom properties with light/dark on `.light`/`.dark`; every `.ts`/`.tsx` carrying the SPDX header; `npm run build` outputs `dist/`.

- [ ] **Step 1:** Scaffold per the kylins versions (react 19, vite 7, tailwindcss 4, zustand, `@tauri-apps/api` 2; dev deps typescript, vitest, @testing-library/react, jsdom added now for Task 13).
- [ ] **Step 2:** Port the theme token set (colours, surfaces, `--header-h`, z indices) trimmed to what the shell needs.
- [ ] **Step 3:** `npm run build` green; remove the Task 9 `.gitkeep`; gate green (license-headers now scans `.ts`/`.tsx`); commit `feat: scaffold the react spa with theme tokens`.

### Task 12: React shell components

**Files:**
- Create: `client/web/src/components/layout/{AppShell.tsx, TitleBar.tsx, StatusBar.tsx}`, `client/web/src/components/folder-pane/{FolderPane.tsx, FolderTree.tsx}`, `client/web/src/components/ui/WindowTitleControls.tsx`, `client/web/src/stores/{accountStore.ts, folderStore.ts, uiStore.ts}`, `client/web/src/services/{ipc.ts, surfaces.ts}`, `client/web/src/events/{surfaceEvents.ts, surfaceEvents.test.ts}`, `client/web/src/features/account-setup/{AccountSetupFlow.tsx, AccountSetupFlow.test.tsx}`
- Port from: `KY/kylins.client.frontend/src/components/layout/{AppShell.tsx, TitleBar.tsx}`, `ui/WindowTitleBar.tsx` (controls), stores and services patterns, `events/engineEvents.ts` (typing pattern).

**Interfaces:**
- Produces: `ipc.ts` exposing `dispatch(intent)`, `mailboxList()`, `syncProgress()`, `addAccount(toml)`, `imapTestConnection(cfg)` as typed wrappers; `surfaceEvents.ts` listening to `surface_changed` and dispatching to stores; FolderPane rendering `MailboxListSnapshot` (accounts, nesting, unread badges, roles) from a `__fixtures__` JSON in tests; the account setup flow collecting host/port/user/password, testing the connection, calling `addAccount` then `dispatch({ RefreshMail: null })`; window controls wired to `getCurrentWindow()`.

- [ ] **Step 1:** Write `surfaceEvents.test.tsx` and `AccountSetupFlow.test.tsx` first (mocked ipc): a `surface_changed` MailboxList event triggers a `mailboxList` pull into `folderStore`; the flow rejects on failed `imapTestConnection` and calls `addAccount` + `RefreshMail` on success.
- [ ] **Step 2:** Implement components and stores; drag regions and double-click maximise per kylins TitleBar.
- [ ] **Step 3:** `npm run build` and `npx vitest run` green; gate green; commit `feat: add the desktop shell (titlebar, folder pane, status bar) and account setup flow`.

### Task 13: Vertical slice acceptance and docs

**Files:**
- Create: `scripts/dev/slice-check.sh`, `docs/changelog/unreleased/sp1-vertical-slice.md`
- Modify: `docs/capabilities.md` (+ zh mirror), `docs/zh/...` mirrors for any doc touched, `.github/workflows/ci.yml` (desktop job)

- [ ] **Step 1:** Write `slice-check.sh`: harness up, app under test boots, adds `astra1@example.test` via the ipc seam (a `--headless-slice` flag on the desktop binary exercising `add_account` + `dispatch(RefreshMail)` + polling `mailbox_list`, printing the folder tree as JSON), asserts INBOX and the custom folder appear, restarts the binary, asserts the tree persists from `preferences.toml` + engine store. This headless mode is a real `main.rs` flag, not test-only code duplication.
- [ ] **Step 2:** Run it green; record output in the report.
- [ ] **Step 3:** Update `capabilities.md`: accounts rows (wizard partial: manual IMAP only), folder pane rows (tree, expansion persist) to partial; CLI/AI rows untouched. Mirror to zh. Add the changelog fragment (`Platforms: desktop`, `Bump: minor`).
- [ ] **Step 4:** CI: add a `desktop` job (windows-latest building `client/desktop` via `cargo clippy -p astra-desktop` + `cargo build -p astra-desktop`; ubuntu keeps the core-only default-members set); nightly unchanged this SP.
- [ ] **Step 5:** Full gate + `cargo test --workspace` + frontend build/tests; commit `feat: prove the vertical slice end to end and update capabilities`.

### Task 14: SP1 close-out

- [ ] **Step 1:** `cargo run -p xtask -- gate`, `cargo test --workspace`, `cargo build -p astra-desktop`, `cd client/web && npm run build && npx vitest run` all green; working tree clean.
- [ ] **Step 2:** Branch `sp1-core-architecture` pushed, PR opened against main with the spec linked; CI green on the PR.
- [ ] **Step 3:** Ledger and workspace cleanup per the SDD skill; report the six acceptance criteria of the spec section 9 with evidence for each.

---

## Self-review notes

- Spec coverage: D1 (Tasks 9-10), D2 (Tasks 5, 10), D3 (Task 6), D5 (Tasks 2-6, 9-12), D6 (Task 7), harness and acceptance (Tasks 8, 13), CI scope (Task 13.4). D4 needs no task: it is an absence.
- Interface consistency: `FolderRef` defined in Task 5 is consumed by Tasks 6 and 12; `MailboxListSnapshot` defined in Task 2 flows through Tasks 6, 10, 12; `ImapConnections` from Task 4 feeds Tasks 6, 8, 10.
- Known risk carried as open items: pedantic clippy inside the Tauri crate may need a scoped relaxation (Task 10 records it as a ruling if taken).
