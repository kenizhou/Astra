# Pinned searches design

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** The search island keeps a session history of recent queries; there is
no way to keep a query for later. This kylins addition adds pinned (saved)
searches: save the live query under a name, list the saved searches, run one,
rename, delete. It is the quick-steps store shape (zustand + localStorage,
defensive parse, logged-not-thrown persist) pointed at the already-shipped
Intent::Search. Pure client/web: no engine, no host, no viewmodel, no new wire
type. Device-local and capped. No eval/pipeline bump.

## D1: the saved-search store

A new client/web/src/features/pinned-searches/pinnedSearchesStore.ts (cloning
quickStepsStore.ts): a zustand store over localStorage key astra.pinnedSearches,
a capped list (20) of { id, name, query } entries, defensive parse (a malformed
entry is dropped, never thrown), storage failure logged never thrown. CRUD:
save(name, query), rename(id, name), remove(id), and the list. The query string
only is saved, never the scope: search.md rule 7 pins the scope as session state
never persisted, so a saved search is a query, not a narrowing. A save with a
blank query is refused (there is nothing to pin). Duplicate names are allowed
(the id is the identity); a save of an identical query+name is idempotent.

## D2: the panel + the save affordance

A new PinnedSearchesPanel.tsx (list / run / rename / delete, an empty state)
mounted in TitleBar.tsx beside the notification-history bell pattern. Running a
saved search opens the search island (openSearchIsland) and dispatches
intents.search(query); the scope opens at the default, exactly as a typed search
does (the saved query carries no scope). A "Save search" affordance on
AmbientIsland.tsx captures the live query under a name when a query is live (and
is absent when the field is blank). Optional palette commands in commands.ts
(open the panel). No ipc.ts change (at the cap); the feature owns its types.

## D3: the contract

docs/search.md gains a "Saved searches" section (the query-only persistence, the
scope-not-saved rule, the cap, the session-history distinction: the island's
recent history is session state, pinned searches are its persistent sibling).
docs/capabilities.md gains a row (naming the query-only scope and the
session-history distinction). A changelog fragment lands. docs/porting-backlog.md
moves "pinned searches" into the shipped list. The zh mirrors move in the same
commit.

## Known gaps recorded, not hidden

- Query-only: the scope is not saved (search.md rule 7), so a pinned search
  re-runs at the default scope, not the narrowing it was saved under.
- Device-local: localStorage, no cross-device sync.
- A saved query re-runs against the current sync depth, so results may differ
  from when it was saved (the horizon note already states the depth policy).

## Test-first

Every behaviour lands with its red. Store tests (parse drops malformed entries,
the cap, save/rename/remove CRUD, blank-query refusal, idempotent save, persist
failure logged not thrown) mirroring quickStepsStore.test.ts. Panel component
tests (run dispatches intents.search with the saved query, rename/delete, the
empty state) mirroring QuickStepsManager.test.tsx. An island test that "Save
search" captures the live query and is absent when blank. The vitest traps apply:
wrap mid-test store mutations in act(), mock outer functions whole. The offline
battery is the proof (a pure web feature; no live harness proof).