# SP2g Search Design

Date: 2026-10-11
Status: proposed
Parent spec: `docs/superpowers/2026-10-08-astra-ai-driven-workflow-design.md` (sections 5, 15)
Port contract: mail-calendar `docs/search.md` in substance.

## 1. Purpose

Find mail: the query DSL (`from:`/`to:`), newest-first results grouped by
thread, the scope filter, the sync-depth disclosure, and the exit back to
the folder view. The engine's search surface serves; Astra renders.

## 2. Decisions

- **D1**: two Intents join the protocol: `Search { query: Option<String> }`
  (None clears, returning to the folder view) and `SetSearchScope {
  scope: SearchScope }` (per MC: all folders except Trash by default, the
  current folder, or the current account). The mailbox snapshot grows
  additively: `search: Option<SearchState>` where SearchState carries the
  result rows (thread-grouped, newest-first), the active scope, the depth
  note (the searched horizon in whole months), and the query echo. Rows in
  search mode reuse the message-row view type.
- **D2**: query handling splits honestly: the engine's `search_mail` owns
  matching (protocol and text search belong to the engine); the APP parses
  the `from:`/`to:` prefix filters into the engine's query parameters
  (read the engine's search signature at the pinned rev first and adapt;
  if the engine accepts structured filters, pass them; if not, the filters
  narrow results app-side; record whichever ships). The free-text
  remainder passes through.
- **D3**: the depth disclosure is honest data: the searched horizon (the
  account's synced window, in whole months, MC's three-month default rides
  the existing sync-depth) renders in the search state; results never
  claim beyond it.
- **D4**: the search box sits in the title bar's centre (the kylins
  Ambient pattern adapted): typing debounced (500 ms) dispatches Search;
  Escape clears (dispatches Search None); the scope filter rides the box
  as a dropdown. The message list renders the result rows with the thread
  grouping and the depth note above; selecting a result opens the message
  in the reading pane as usual.
- **D5**: thread grouping: results group by thread (the engine returns
  messages; the app groups by thread id into the row view with counts),
  newest thread first, matching MC's thread-grouped results contract.

## 3. Testing

- Unit (astra-app, in-memory engine + fakes): Search(Some) publishes the
  search state and suppresses the folder list state (additive snapshot
  fields, the renderer picks); Search(None) returns; SetSearchScope
  re-runs; the DSL split (from:/to: extraction, the free-text remainder,
  the malformed-prefix tolerance); the depth note (the synced window
  quantised); thread grouping and ordering.
- Wire: the two intents and the search state literals pinned both sides.
- Frontend (vitest): the box (debounce, Escape, the scope dropdown), the
  results render (grouped rows, the depth note, the count), selection
  opens the reader, exit returns to the folder list.
- Live (gated, harness): a real search over the seeded notes (subject
  hit; from: filter hit; no-hit state) through the real engine.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build, vitest,
   sidecar tests green.
2. Offline: the full search lifecycle pinned (type -> results -> scope
   change -> select -> exit); the DSL split pinned; the depth note pinned.
3. Live: real searches hit the seeded notes through the harness.
4. `docs/search.md` (+ zh) ported in substance (the DSL, the scope set,
   the grouping, the depth disclosure, the exit), capabilities rows moved
   with mirrors, a changelog fragment.
5. Additive wire only; existing fixtures and tests pass unchanged.
