# Categories: the Astra Mail contract

**Scope.** Labelling mail with a colour category: define a coloured category,
assign it to conversations, see a colour bar on the rows, filter by category,
and manage the definitions. Binding on every surface that ships the feature.
This is a LOCAL overlay, the locked kylins reference's red line
(`thread_categories.rs`): categories are local v1, no engine write, no sync, no
protocol concept. EAS and IMAP accounts are served identically because the
tables are protocol-agnostic shell state. No engine change; no eval or pipeline
bump (manual categories are not AI). The gaps are recorded at the end, never
left silent.

**Principle.** *A category owns no engine state.* The definitions and
assignments live in a device-local store; the list reads them back through a
post-projection fold. Assigning a flat message row resolves its thread key
through one engine READ, never a write. The engine's keywords are never
touched: a category is not a keyword.

## The store

A new crate `astra-categories` holds the definitions and assignments (the
reminder store's shape: one rusqlite connection under a mutex, embedded
migrations claimed in a transaction, the five-second busy timeout, an
in-memory open for tests). The host opens `categories.sqlite` in the data
directory and installs it on the core (a `Mutex<Option<Arc<Store>>>` slot; no
`AppState` field and no timer, so the drop order stays as it stands). Two
tables:

- `categories`: the definitions, global (not per-account). `id` (the name's
  slug, the primary key), `name` (unique, `COLLATE NOCASE`), a `#RRGGBB`
  colour, and `created_at`. Listed in creation order (rowid); a rename or
  recolour never reorders.
- `thread_categories`: one row per (account, thread, category). `account_id`,
  `thread_id`, `category_id`, `created_at`, primary key
  (account_id, thread_id, category_id), and a `FOREIGN KEY category_id
  REFERENCES categories ON DELETE CASCADE` with an index on `category_id`.

**The foreign-key pragma is load-bearing.** rusqlite does NOT enable foreign
keys by default; the store sets `PRAGMA foreign_keys = ON` in BOTH `open` and
`open_in_memory` (the crypto store's precedent). Without it the delete cascade
silently no-ops and dangling assignments survive. A store test proves the
cascade fires.

A `CategoryId` newtype carries the slug across the boundary into the row wire.
The methods mirror the reference: `create_category` (validate the trimmed name
at most 64 characters and the `#RRGGBB` colour, mint the slug with `-2`, `-3`
collision suffixes, a typed NOCASE "already exists" refusal), `update_category`
(rename and recolour in place; the assignments key the id so they survive),
`delete_category` (a transactional count plus the cascade, answering the
removed count), `assign_category` (probe exists then `INSERT OR IGNORE`,
idempotent, `Ok(true)` iff inserted), `unassign_category` (idempotent),
`list_categories` (rowid order), and `assignments_for_threads` (a bound `IN`
query, `ORDER BY thread_id, rowid`, account-scoped, answering a map of thread
id to category ids). The store is device-local: it does not roam across
devices, and the CLI does not share it (the snooze/reminders precedent).

## The wire and the overlay

`MessageRowView` gains `categories: Vec<CategoryId>` (serde default, skipped
when empty, the `thread_count` idiom): ids only, the names and colours
resolving surface-side from the definitions snapshot. The three `list_rows`
constructors initialise it empty; the pure projection stays untouched.

A new `categories_overlay` folds the assignments AFTER projection: for each
row it resolves the thread key (a thread row by its id, a message row by its
source row's `thread_key`, the resolved thread id or the message's own provider
key when never threaded), groups by account, runs one `assignments_for_threads`
query per account on the page, and sets `row.categories`. The fold runs at the
three projection call sites (the folder rows, the unified rows, and the search
results). The overlay is NEVER cached: the mail-row cache holds engine rows
only, and the fold runs fresh on every rebuild.

## The core verbs

`install_categories_store` installs the store. Six verbs: `create_category`,
`update_category`, `delete_category`, `assign_category`, `unassign_category`,
and `list_categories`. Assign and unassign take a `RowRef`: a thread row keys
its id directly; a message row resolves its thread key through one engine read
(`mail_by_keys`), the same `thread_key` convention the projection groups by, a
READ so the red line holds. Assign, unassign, and delete end with
`invalidate_list_cache`, `bump_generation`, and `rebuild_snapshot` so the rows
carry or shed the id; create and update need no rebuild (the wire carries ids
only). An app with no store overlays nothing and refuses writes; the listing
answers empty.

## The host commands

A `commands/categories.rs` submodule carries the six commands (the
`commands/reminders.rs` shape: an inner `*_of` body and a thin
`#[tauri::command]` wrapper; `commands.rs` is at the cap). A malformed row
reference is refused before the core. The `AppPort` gains the six methods
through a `CategoriesPort` supertrait in its own module
(`app_state/categories_port.rs`, `trait AppPort: CategoriesPort`), so each
impl's category block lives in its own child file (one trait's impl cannot
split across modules; distinct traits impl freely), leaving `app_state.rs` and
the fake port nearly untouched. The boot opens `categories.sqlite` and installs
it in `assemble_state`; no timer, the drop order untouched.

## The surface

The wire rides `services/categories.ts` (the `CategoryDef` type, the six
wrappers, and the twenty-swatch palette; never `ipc.ts`, at the cap). A
`categoriesStore` holds the definitions snapshot, a `loadCategories` loader
mounted once in `MessageList`, and a revision bump. The manager dialog
(`CategoriesDialog`) offers the name and the palette radiogroup, a rename and
recolour edit, and a delete confirm carrying the removed-from-every-conversation
sentence and the count; the local-only copy stands up front. The row menu's
Categorize submenu (`categoryMenu.ts`, the `blockSenderMenu` pattern) lists the
definitions as checked swatches with the Outlook toggle: when EVERY target
carries the category remove it from all, else add it to the missing; a small
`ContextMenu` child-renderer extension draws the checked gutter and the swatch
and keeps the menu open across picks (`keepOpenOnSelect`). A leading colour bar
on `MessageRow` wears the first category's colour, the tooltip naming every
category the row carries; a deleted definition resolves to nothing. A category
filter (a `listFilter` predicate, a `listFilterStore` `categoryFilter`, and a
`ListToolbar` select picker outside the tablist) ANDs onto the active tab,
resets on a folder change, clears when its definition is deleted, and stands
disabled with no definitions.

## The SetKeywords trap, declined

The engine's `SetKeywords` path writes SERVER-SYNCED keywords through the
outbox. Riding it for categories would violate locality, not capability: the
engine can write keywords, but a category must never leave the device, never
sync, and never become a protocol concept. The local overlay is the faithful
port; the keyword path is the trap this slice deliberately avoids.

## Known gaps

- **Classification** (AI auto-categorise) is a separate follow-up (it triggers
  the eval and pipeline-bump rule); **conditional formats** are a separate
  follow-up (they read categories but are their own contract).
- **Local-only**: no sync, no cross-device roam, no protocol concept; the
  definitions are global (not per-account) and creation order is fixed (no
  reordering).
- **No CLI/agent verb** (local view state); **no custom colour picker** (the
  palette only; the backend accepts any `#RRGGBB`).
- **Batch assign loops per row** (the row-menu compromise); a multi-account
  batch resolves per row.