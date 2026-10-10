# A3 Knowledge, Snippets, Autocomplete, Translate Design

Date: 2026-10-11
Status: proposed
Parent spec: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (section 9.3)
Consumes: the A1/A2 skeleton (client, seams, judges, ledgers, eval harness), the reading pipeline, the composer.

## 1. Purpose

The assistant's memory and reach: extracted knowledge entries (facts,
preferences, ongoing threads of life, persona notes) that shape drafts; a
snippet library with AI search; a compose autocomplete suggestion; and a
translate action for the reading pane.

## 2. Decisions

- **D1 (knowledge store and extraction)**: migration `0007_knowledge`
  (id, account, kind: fact | preference | project | contact | persona,
  text, source_message, created_at, correlation_id). Extraction is a batch
  job: a host command `knowledge_extract(account, limit)` runs the
  structured extraction over the account's N most recent messages (N capped
  at 30), each message untrusted-wrapped, producing zero or more entries
  with their source tracked (`KNOWLEDGE_EXTRACT_VERSION`, ECONOMY tier).
  An exact-text dedup guard refuses duplicate entries. Entries never
  auto-delete; `knowledge_forget <id>` removes (CLI + host). The draft
  prompt (A2) grows the account's knowledge entries as labelled data on
  the trusted half, capped at the newest ten by kind-relevance order
  (persona + preferences first, then facts/projects), itself versioned as
  a `DRAFT_REPLY_PROMPT_VERSION` bump.
- **D2 (snippets)**: migration `0008_snippets` (id, title, body, created_at,
  updated_at). CRUD through the CLI (`snippets list|create|delete`, the
  agent-first surface) and the composer's picker; search is ONE structured
  ranking call (`SNIPPET_SEARCH_VERSION`, ECONOMY tier) over titles and
  bodies returning matched ids in order, falling back to substring search
  on failure or empty store. The composer picker lists snippets (recent
  first) with a search box; inserting places the body at the cursor.
- **D3 (autocomplete)**: a host command `compose_autocomplete(before,
  after)` (the editor's text around the cursor, the trailing context
  optional) returning one short continuation (`AUTOCOMPLETE_VERSION`,
  CHAT tier for latency; the spend guard applies). The editor surfaces the
  suggestion as a dimmed inline chip under the current line with a Tab or
  click to insert; the chip never inserts without the gesture. Debounced
  idle triggering (two seconds after typing stops, unchanged-text skip);
  a dismiss for the session.
- **D4 (translate)**: a reading-pane action translating the open message
  into a chosen target language (a small fixed list: English, Chinese,
  German, French, Japanese, Spanish; default English), served by a
  completion call (`TRANSLATE_VERSION`, DEFAULT tier) with the body
  untrusted-wrapped, cached in migration `0009_text_cache` keyed (message
  account, message key, kind=translate, target language, body hash). The
  result renders in the pane as a collapsible block beside the summary;
  cache hits make no call.
- **D5 (wire and CLI)**: host commands `knowledge_list`,
  `knowledge_extract`, `knowledge_forget`, `snippet_search`,
  `snippet_list`, `compose_autocomplete`, `translate`; the CLI verbs read
  the stores (the mutating CRUD verbs per the contract with `--preview`);
  extraction, search, autocomplete, and translate never run from the CLI
  (the never-dials rule).

## 3. Testing

- Unit (astra-app/astra-ai, fakes + fixture seams): extraction shape
  (entries + sources + dedup + the cap), the knowledge-in-prompt cap and
  ordering, snippet search (ranking, empty store fallback, failure
  fallback), autocomplete (before/after context, the version const, the
  unchanged-skip), translate (cache hit no-call, invalidation on
  language/body change).
- Fixture servers: the ECONOMY/CHAT/DEFAULT tier wire shapes.
- Frontend (vitest): the snippet picker (list, search, insert at cursor),
  the autocomplete chip (debounce, dismiss, insert-on-gesture only, no
  insert on re-render), the translate block (language pick, cached,
  fresh, error).
- Live (gated, the provided endpoint + harness): a real extraction over
  the seeded notes (entries land with sources); a real snippet ranking; a
  real autocomplete suggestion; a real translate with the cached second
  call making no network request.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build, vitest,
   sidecar tests green.
2. Offline: the full lifecycle of each feature pinned (extract -> store ->
   prompt shaping; snippet CRUD -> search -> insert; autocomplete debounce
   -> chip -> insert; translate -> cache).
3. Live: extraction, snippet ranking, autocomplete, and translate run
   against the endpoint with sane costs; the cached translate second call
   makes no request.
4. `docs/ai-assist.md` (+ zh) with the contracts and known gaps,
   capabilities rows moved with mirrors, fragments, and eval fixtures with
   version pins for every new prompt constant.
5. Additive wire only; existing fixtures and tests pass unchanged.
