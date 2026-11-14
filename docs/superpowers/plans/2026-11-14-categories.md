# Categories plan

Spec: docs/superpowers/specs/2026-11-14-categories-design.md. Branch `categories`
from main at the spec commit. Each task: reds first, implement, battery, one
conventional commit. One contract change: a new docs/categories.md with its zh
mirror in the same commit, the capabilities row moves with its zh mirror, a
changelog fragment lands, and porting-backlog.md moves categories into the shipped
list. No engine change, no AI bump. Faithful local-overlay port, no reference
deviation.

### Task 1: the store + the wire + the core overlay (D1, D2)

**Files:** a new crate crates/astra-categories (Cargo.toml + src/lib.rs +
src/store.rs + src/migrations/0001_init.sql, the astra-reminders shape; the
categories + thread_categories tables, FK ON DELETE CASCADE, PRAGMA foreign_keys ON
in BOTH opens; CategoryId newtype; create/update/delete/assign/unassign/
list_categories/assignments_for_threads, slug_for/valid_color/validate) + store
tests; crates/astra-viewmodel/src/rows.rs (MessageRowView.categories Vec<CategoryId>
serde default + skip_serializing_if empty) + Cargo.toml dep; crates/astra-app/src/
categories.rs (install_categories_store + six core verbs, assign/unassign/delete
rebuild) + categories_overlay.rs (the post-projection fold) + the three list_rows
constructors init categories + the fold call sites (snapshot_rows folder_rows/
unified_rows, search run_search) + overlay tests; the App slot (lib.rs at cap,
reclaim doc lines).

- [ ] Confirm at the hook site: the astra-reminders store shape, the FK pragma
      precedent (astra-crypto-store/store.rs:137), the thread_key convention
      (list_rows.rs:309-314), the projection call sites (snapshot_rows.rs:33/70,
      search.rs:189), the rebuild (snapshot.rs), install_reminder_store. Reds first
      (store: CRUD, slug suffix, NOCASE uniqueness, typed refusals, rename preserves
      assignments, delete cascade + count proving FK ON, idempotent assign/unassign,
      unknown-category refusal, assignments rowid order + empty + account scoping;
      overlay: assign projects folder/unified/search BOTH flat + threaded incl.
      lone-message, unassign removes, delete cascades, rename keeps ids, ENGINE
      UNTOUCHED zero edit_mail, no-store empty); then the store + wire + core.
      Commit `feat: the category store and overlay`.
- [ ] astra-categories/astra-app/astra-viewmodel NOT cap-exempt (lib.rs at cap
      reclaim, list_rows +3 safe). Newtypes (CategoryId crosses the boundary). The
      FK pragma is load-bearing (cascade silently no-ops without it). No engine
      write. Existing tests stay green.

### Task 2: the host commands + AppPort + boot (D3 host)

**Files:** a new client/desktop/src/commands/categories.rs (six commands, the
commands/reminders.rs shape, malformed-ref refusal; commands.rs at cap reclaim a
line) registered in lib.rs; a new app_state/categories_port.rs (CategoriesPort
supertrait, trait AppPort: CategoriesPort) + each impl's category block in its own
child file (app_state/port.rs, commands_fakes/port.rs near cap, showcase/app.rs);
the boot install (boot.rs assemble_state opens categories.sqlite +
install_categories_store, CATEGORIES_STORE_FILE const, no timer).

- [ ] Reds first (command routing + malformed-ref refusal over the fakes); then the
      commands + supertrait + boot. Commit `feat: the category host commands`.
- [ ] client/desktop NOT cap-exempt: the supertrait split keeps app_state.rs (500)
      and commands_fakes/port.rs (498) nearly untouched (E0119: distinct traits impl
      in distinct files freely). commands.rs reclaim 1 line. The drop order
      untouched (no timer, no AppState field). Existing tests stay green.

### Task 3: the web surface + contract + live proof (D3 web, D4)

**Files:** a new client/web/src/services/categories.ts (CategoryDef wire + six
wrappers + palette, never ipc.ts); features/categories/categoriesStore.ts (zustand
defs snapshot + loadCategories + revision, one loader in MessageList) +
CategoriesDialog.tsx (manager CRUD, name + 20-swatch palette, delete confirm,
local-only copy, useDialogFocus) + categoryMenu.ts (Categorize submenu, the Outlook
toggle) + tests; a small ContextMenu.tsx child-renderer extension (checked gutter +
swatch + keepOpenOnSelect); the MessageRow.tsx leading colour bar; the category
filter (listFilter predicate + listFilterStore categoryFilter + ListToolbar select
picker, ANDs onto the tab, reset on folder change, clear on def delete); surfaces.ts
MessageRowView.categories; docs/categories.md + zh; docs/capabilities.md row + zh;
docs/changelog/unreleased/categories.md (Platforms: desktop, Bump: minor);
docs/porting-backlog.md + zh; crates/astra-app/src/tests_live_harness/
categories_proof.rs (submit a note, create + assign via the core, assert the row
carries the id over a real read, engine untouched, delete -> row empty, withdraw) +
tests_live_harness.rs registration.

- [ ] Reds first (service wrappers, defs store, toggle semantics, manager CRUD +
      delete confirm, badge first-colour/tooltip/deleted-def, filter ANDing + reset +
      clear-on-delete; the offline-skipping gated live proof); then the web + docs.
      Commit `feat: the category surface and contract`.
- [ ] client/web not exempt: rowMenu.ts 495 (+3 -> 498 tight, the categoryMenu
      builder pattern); ipc.ts at cap (services/categories.ts); ContextMenu.tsx 311
      (+20 safe). vitest traps: act() mid-test, mock the service whole. Comments
      present tense; no em/en dashes; British English; source files end with a
      newline. docs/zh mirrors same commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix sidecar
      test; npx tsc --noEmit && npx vitest run (client/web); cargo run -p xtask
      -- gate. If the plugin tests fail with AlreadyGranted, clean the stale
      /tmp/astra-desktop-plugin-* scratch dirs (Windows pid-recycle flake) and
      re-run. WATCH DISK SPACE (clean the cargo incremental cache if it balloons).
- [ ] Controller arms the live proof on a clean harness (scripts/dev/harness.sh
      reset, then up + env; cargo test -p astra-app
      tests_live_harness::categories_proof -- --test-threads=1), then unset.
- [ ] Push the branch; PR (squash expected); CI watch; squash merge, delete the
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's store + D2's wire/core/overlay (T1); D3's host commands (T2);
  D3's web + D4's contract + live proof + battery + PR (T3).
- Risks: the FK pragma OFF by default (cascade silently no-ops; enable in both opens
      + test); the flat-mode thread-key resolution must match thread_key() exactly
      (lone messages keyed by their own provider key; test both modes); the cap
      pressure (app_state.rs 500 / commands_fakes/port.rs 498 -> supertrait split;
      lib.rs/commands.rs reclaim; rowMenu.ts 495 +3); the local-only red line (no
      engine write; the SetKeywords trap is locality not capability). Classification
      / conditional formats / CLI verb / custom colour / reordering (recorded gaps).
- No eval/pipeline bump (manual categories are not AI). No reference deviation.
      Live-proof lesson: reset the harness first; clean stale plugin scratch dirs if
      the battery flakes; ensure trailing newlines.