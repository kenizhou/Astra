# Categories design

**Status.** Decided 2026-11-14. Slices branch from main at this commit.

**Scope.** There is no way to label mail with a colour category. This kylins
addition adds manual categories: define a coloured category, assign it to
conversations, see a colour bar on the rows, filter by category, manage the defs.
It is a LOCAL overlay, the locked kylins reference's red line (thread_categories.rs):
categories are local v1, no engine write, no sync, no protocol concept; EAS and
IMAP served identically because the tables are protocol-agnostic shell state. A
new astra-categories store (defs + per-(account,thread) assignments), a categories
field on the row wire (ids only), an overlay folded into the list projection, and
the manager/menu/badge/filter UI. No engine change; no eval/pipeline bump (manual
categories are not AI). Faithful port, no reference deviation.

## D1: the category store

A new crate crates/astra-categories (the astra-reminders shape: rusqlite under a
Mutex, embedded migrations, open with the 5s busy timeout, open_in_memory). Two
tables: categories (id TEXT PRIMARY KEY [slug], name TEXT NOT NULL UNIQUE COLLATE
NOCASE, color TEXT NOT NULL [#RRGGBB], created_at) and thread_categories
(account_id, thread_id, category_id, created_at, PRIMARY KEY (account_id,
thread_id, category_id), FOREIGN KEY category_id REFERENCES categories ON DELETE
CASCADE), an index on category_id. CRITICAL: rusqlite does NOT enable foreign keys
by default; set PRAGMA foreign_keys = ON in BOTH open and open_in_memory (the
astra-crypto-store precedent) and a test must prove the cascade fires. A CategoryId
newtype (the slug crosses into astra-viewmodel). Methods (mirror the reference):
create_category (validate name <=64 trimmed + colour #RRGGBB, slug with -2/-3
collision suffix, typed NOCASE "already exists" refusal), update_category
(rename/recolour in place, assignments key the id so they survive), delete_category
(transactional count + cascade, returns the removed count), assign_category (probe
exists + INSERT OR IGNORE, idempotent, Ok(true) iff inserted), unassign_category
(idempotent), list_categories (rowid/creation order), assignments_for_threads
(bound IN, ORDER BY thread_id, rowid, account-scoped). Device-local; the CLI does
not share it (the snooze/reminders precedent).

## D2: the core verbs + the overlay + the wire

A core install_categories_store (the install_reminder_store shape, a
Mutex<Option<Arc<Store>>> slot; lib.rs at the cap, reclaim doc lines) and six core
verbs in a new categories.rs. assign/unassign take a RowRef (Thread -> thread_id
directly; Message -> one engine read to resolve thread_id-or-key, the thread_key
convention list_rows.rs:309-314, a READ not a write so the red line holds);
assign/unassign/delete end with invalidate_list_cache + bump_generation +
rebuild_snapshot (create/update need no rebuild, the wire carries ids only).

A new categories_overlay.rs folds the assignments after projection (the pure
projection stays untouched): for each row resolve its thread key (RowKey::Thread ->
thread_id; RowKey::Message -> the source row's thread_key), group by account, one
assignments_for_threads query per account on the page, set row.categories. Folded
at the three projection call sites (snapshot_rows folder_rows + unified_rows,
search run_search). The overlay is never cached (the mail_row_cache caches engine
rows only; the fold runs fresh on every rebuild).

The wire: MessageRowView gains categories: Vec<CategoryId> (serde default +
skip_serializing_if empty, the thread_count idiom; ids only, names/colours resolve
web-side from the defs snapshot). astra-viewmodel depends on astra-categories
(acyclic). The three list_rows constructors initialise categories: Vec::new().

## D3: the host commands + the web surface

New commands/categories.rs (six commands, the commands/reminders.rs shape,
malformed-ref refusal before the core; commands.rs at the cap, reclaim a line).
AppPort gains the six methods via a CategoriesPort supertrait in its own module
(app_state/categories_port.rs, trait AppPort: CategoriesPort) so each impl's
category block lives in its own child file (E0119 forbids splitting ONE trait's
impl, distinct traits impl freely), leaving app_state.rs (500) and
commands_fakes/port.rs (498) nearly untouched. Boot opens categories.sqlite +
install_categories_store in assemble_state (no timer, the drop order untouched).

The web: services/categories.ts (the CategoryDef wire type + six wrappers + the
palette, never ipc.ts); a features/categories/categoriesStore.ts (zustand defs
snapshot + loadCategories + a revision bump, one loader mounted in MessageList); a
CategoriesDialog manager (name + 20-swatch palette radiogroup, delete confirm with
the removed-from-every-conversation copy, the local-only copy up front, the
snoozeStore open/close shape, useDialogFocus); a Categorize submenu (categoryMenu.ts
the blockSenderMenu pattern, the Outlook toggle: when EVERY target carries the
category remove from all, else add to the missing; a small ContextMenu child-renderer
extension for the checked gutter + swatch + keepOpenOnSelect); a leading colour bar
on MessageRow (the first category's colour, title lists all names, deleted defs
resolve to nothing); a category filter (listFilter predicate + listFilterStore
categoryFilter + a ListToolbar select picker outside the tablist, ANDs onto the
active tab, reset on folder change, clear on def delete, disabled with no defs).

## D4: the contract

A new docs/categories.md (the local-only red line, the store, the overlay, the
surface, the recorded gaps; the SetKeywords trap rationale: the engine path writes
SERVER-SYNCED keywords through the outbox, violating locality, not capability) +
zh; the capabilities row; a changelog fragment; the porting-backlog shipped-list
move. The zh mirrors move in the same commit.

## Known gaps recorded, not hidden

- Classification (AI auto-categorise) is a separate follow-up (triggers the
  eval/pipeline-bump rule); conditional formats are a separate follow-up (they read
  categories but are their own contract).
- Local-only: no sync, no cross-device roam, no protocol concept; defs global (not
  per-account), creation order fixed (no reordering).
- No CLI/agent verb (local view state); no custom colour picker (palette only, the
  backend accepts any #RRGGBB).
- Batch assign loops per-row (the row-menu compromise); multi-account batches
  resolve per row.

## Test-first

Every behaviour lands with its red. Store tests (CRUD, slug collision suffix, NOCASE
uniqueness, typed validation refusals, rename/recolour preserves assignments, delete
cascade + removed-count proving FK pragma ON, idempotent assign/unassign,
unknown-category refusal, assignments_for_threads rowid order + empty page + account
scoping). Core overlay tests over the fake provider (assign projects onto
folder/unified/search rows in BOTH flat and threaded modes incl. the lone-message
case, unassign removes, delete cascades off rows, rename keeps ids, ENGINE UNTOUCHED
[zero edit_mail calls], no-store answers empty). Command tests (routing +
malformed-ref refusal). Web tests (service wrappers, defs store, the toggle
semantics, manager CRUD + delete confirm, badge first-colour/tooltip/deleted-def,
filter ANDing + reset + clear-on-delete; act() mid-test). ONE gated live harness
proof (categories_proof.rs: submit a note, create + assign a category via the core,
assert the mailbox_list row carries the id [overlay over a real read], assert the
engine untouched [keywords unchanged, no outbox op, note in INBOX unmoved], delete
the category, assert the row's categories empty, withdraw the note). The offline
battery is the primary proof; the live proof adds the overlay-over-real-read +
engine-untouched assertion.