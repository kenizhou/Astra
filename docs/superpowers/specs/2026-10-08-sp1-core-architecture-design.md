# SP1 Core Architecture Design

Date: 2026-10-08
Status: proposed (awaiting review)
Parent spec: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (sections 4, 5, 15)

## 1. Purpose

SP1 builds the core architecture and proves it with one vertical slice: add an
IMAP account, sync it, see its folder tree in the desktop shell. Everything
later (SP2+ feature slices, SPA0 AI foundation) grows inside the seams this
spec fixes: the Intent/Surface protocol, the crate boundaries, the Tauri host
ports, the IPC contract, and the plugin host skeleton.

## 2. Reference points (from the deep-dive explorations)

- Engine: `allodia-eu/email-calendar-sync-engine` at rev `68eab00e...`, nine
  crates git-pinned in the workspace (`engine-api`, `engine-core`,
  `engine-provider`, `engine-tls`, `provider-imap`, `provider-caldav`,
  `provider-graph`, `provider-google`, `provider-jmap`); `engine-api::Engine`
  is the facade (`Arc<Engine>`, per-method generic over `P: Provider`);
  ids come from `engine-core` (`object_id!` newtypes) and are re-exported
  through `engine-api`.
- mail-calendar core: `Intent` dispatch (`dispatch(intent).await`, one match
  arm per variant, family arms delegate), `Surfaced<T>` publish-then-signal
  cells, `AppObserver::surface_changed`, snapshot rebuild with generation
  guards and a windowed row cache, engine-owned durable outbox
  (`submit_mail` -> `queued_state` -> `SendOutcome`), boot sequence
  (`Engine::open` -> `abandon_sync_leases` -> `App::new` ->
  `recover_outbox` -> `prime_snapshot` -> background connect and sync).
- kylins shell: frameless Tauri 2 window with custom TitleBar, AppShell
  layout, Zustand stores consuming events, services as thin `invoke()`
  wrappers, typed event mirrors, theme manager with CSS custom properties.

## 3. Decisions

- **D1**: the Tauri host replaces mail-calendar's `mailcal-bindings`: it owns
  the tokio runtime (multi-thread, workers = cores minus one; blocking host
  calls on a dedicated large-stack thread) and implements the host service
  ports. It consumes `astra-app` directly as a crate (the Linux client
  precedent).
- **D2**: `Intent` and `Surface` carry serde derives on Astra (mail-calendar
  crosses FFI as UniFFI records; Tauri IPC needs JSON). The IPC contract:
  a `dispatch(intent)` command, a `surface_changed` event carrying the
  surface name, and one pull command per surface (for SP1:
  `mailbox_list`). The observer bridge debounces like mail-calendar:
  250 ms windows, leading edge for MailboxList, trailing for
  SyncProgress.
- **D3**: sync rounds belong to `astra-app` (join_all over accounts ->
  `engine.sync_mail` -> `drain_outboxes` -> `rebuild_snapshot`); the host
  only owns the runtime and spawns background tasks. No shell-level
  scheduler.
- **D4 (Ruling R7)**: no product SQLite in SP1. Persistence is the engine's
  SQLite plus `preferences.toml` plus the OS keyring, exactly the
  mail-calendar shape for this stage. A sqlx product database arrives with
  SPA0 when the AI ledgers need it. If wrong, the cost is one schema
  migration later, not a rework.
- **D5**: the vertical slice ships the minimal sets both explorations
  identified (sections 5 to 7).
- **D6**: the plugin host lands as a skeleton in SP1: manifest types,
  lifecycle states, the versioned hook API surface as types, and the
  permission model declaration. Out-of-process execution and the
  example-plugin e2e land at the end of SP1 or with SP2, whichever the plan
  judges.

## 4. Crate layout and dependencies

```
crates/
  astra-app/         protocol (Intent, Surface, status), surfaced.rs, dispatch,
                     snapshot + window + row_cache, accounts, sync, boot
  astra-viewmodel/   MailboxListSnapshot, folder rows (leaf: engine-api only)
  astra-account/     AccountConfig (TOML), setup -> build_config_toml, IMAP
                     connections and binding, reconnect wrapper, credential
                     source, TLS, certificate exceptions types
  astra-plugin/      manifest, lifecycle, hook API v1 types, permissions
client/
  desktop/           the Tauri host: runtime, ports (keyring credential
                     store, observer bridge), commands, events, boot
  web/               the React SPA (section 7)
```

`astra-app` depends on `astra-viewmodel`, `astra-account`, `engine-api`,
`engine-provider`, `engine-core`. `client/desktop` depends on `astra-app`
plus Tauri and keyring crates. Workspace dependency table starts from the
mail-calendar set minus UniFFI (tokio sync/time for the core crates; futures;
async-trait; serde/serde_json; toml; thiserror; rustls stack for IMAP;
time; log) and grows only on demand.

## 5. Protocol v1

- Intent variants for SP1: `RefreshMail`, `SelectAccount(Option<AccountId>)`,
  `SetAccountExpanded { account, expanded }`, `SetFolderExpanded { folder,
  expanded }`, `SelectFolder { folder }`, `SetViewMode(ViewMode)`,
  `ShowMore`, plus the family shape (`Folders(FolderIntent)`) with
  `FolderIntent::Create` reserved but not dispatched. All serde with
  externally tagged enums, snake_case, `None` omitted (matches the engine
  event wire style the frontend already mirrors).
- Composite refs in `astra-app::reference` (`FolderRef { account, key }`)
  with `from_parts` returning `None` on malformed halves.
- Surfaces for SP1: `MailboxList` (pushed snapshot) and `SyncProgress`
  (signal-only, pulled). `Surfaced<T>` is ported as-is (publish then signal;
  `get()` clones and never signals).
- Snapshot: `MailboxListSnapshot { account_folders: Vec<AccountFolderRow> }`
  with per-account `expanded`, unread counts, and role icons as data.
- Preferences in `preferences.toml`: view mode, folder expansion state,
  selected folder; loaded at boot, written on the matching intents.

## 6. Tauri host design

- Builder: single-instance first, deep link (`mailto:` reserved), opener,
  dialog, notification; frameless main window with the kylins geometry
  (1200x800, min 800x600). No tray, no autostart, no global shortcuts in
  SP1 (they arrive with the settings slice).
- Commands: `dispatch`, `mailbox_list`, `sync_progress`, plus account
  setup: `add_account(config_toml)` and `imap_test_connection`. The
  server-recommendation step (`detect_account_settings`) is a known
  deferral to the SP2 accounts slice, where the autodetect strategies
  port; the manual-IMAP-first defaults live client-side in the setup
  flow.
- Events: `surface_changed` (name + nothing else; the client pulls),
  reserved names `engine:*` are not used: the app is the only engine
  consumer.
- Credential store port: `AccountCredentialStore` implemented over the OS
  keyring, injected as a required constructor parameter, never a setter;
  add-failure rolls the registration back.
- Exit: `RunEvent::Exit` stops sync with a bounded wait.

## 7. React shell (minimal)

- `AppShell` (TitleBar with drag regions and window controls, FolderPane
  tree, MessageList placeholder, StatusBar), theme manager with the CSS
  custom property pack (light/dark), stores `accountStore`, `folderStore`,
  `uiStore`, services as `invoke()` wrappers, typed event client for
  `surface_changed`, and the account setup flow ending in the first
  `dispatch(RefreshMail)`.
- The plugin slot-injection runtime from kylins is not wired in SP1; the
  `astra-plugin` types are compiled and unit-tested only.

## 8. Testing

- Rust: inline sibling test files per the rulebook; `Engine::open_in_memory`
  drives app-level tests (dispatch, surfaced semantics, snapshot rebuild,
  preferences round trip).
- Live: the Stalwart harness (`scripts/dev/harness.sh`, ported in this SP)
  serves the acceptance run: add an IMAP account against the seeded server,
  observe `MailboxList` publish the folder tree.
- Frontend: Vitest component tests for the FolderPane render from a fixed
  snapshot fixture and the account flow against mocked commands.

## 9. Acceptance criteria

1. `cargo run -p xtask -- gate` green including license headers on every
   new file.
2. Against the Stalwart harness: adding an IMAP account from the wizard
   persists credentials in the OS keyring, syncs via the engine, and the
   folder tree renders in the shell, surviving an app restart.
3. Every mail operation flows through `dispatch`; the account-setup
   commands (`add_account`, `detect_account_settings`,
   `imap_test_connection`) are the sanctioned setup-path exceptions.
4. The engine is consumed only through `engine-api` (plus the ratified
   reach-arounds inside `astra-account`: provider crates, `engine-tls`
   for the TLS policy, and `engine-core` for the Provider trait's own
   SyncState/SyncUpdate/RawMime types; test fakes elsewhere are
   dev-dependencies), pinned at rev `68eab00e`.
5. `capabilities.md` rows for the slice move to partial or complete, with
   the changelog fragment in the same change.
6. `client/desktop` builds on Windows; CI green.

## 10. Out of scope (arrives with SP2+)

Message list contents, reading pane, compose and drafts, outbox and
sending surfaces, calendar, contacts, autodetect strategies beyond the
manual IMAP path, search, MCP, the sqlx product database, and the AI
sidecar.

## 11. Open items

- Whether the plugin host's out-of-process execution lands at the end of
  SP1 or with SP2 (D6; the plan decides by remaining capacity).
- Splashscreen and window theming depth (kylins has both; SP1 takes the
  plain window unless the plan finds them cheap).
