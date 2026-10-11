# AI assist: the Astra Mail contract

**Scope.** The A3 assist features on the A1/A2 foundation: the knowledge
store with its batch extraction and the draft prompt's knowledge context,
the snippet library with its ranked search and the composer's picker, the
compose autocomplete suggestion with its chip, the cached translate
action, and the host commands and CLI verbs that expose them. Binding on
every surface that ships any of it (today the desktop host, the web
composer and reading pane, and `astra-cli`).

**Principle.** *The assist features help the writer; they never act
alone.* Knowledge is model-derived data about the account's owner, kept
as data and never as instruction; a snippet is the writer's own text,
stored whole and inserted only where the cursor sits; an autocomplete
suggestion never inserts without the writer's gesture; a translation is
rendered beside the original, never in place of it. Nothing here sends
anything, and the model-backed runs live in the app: the CLI reads the
stores and refuses the model-class spellings (the never-dials rule,
`docs/cli.md`).

## Knowledge

One row per extracted entry (`ai.sqlite` migration `0007`): the account
the entry belongs to, its kind, its text, the source pair (the account
and provider key of the message it was extracted from), the extraction
turn's correlation id, and the store's stamp. The **closed set of kinds**
is `fact`, `preference`, `project`, `contact`, `persona`; an unknown
kind word in a completion fails the whole parse loudly (the batch lands
nothing, never a guess), and a foreign word already in the store passes
through the listings as it stands.

**The account owns its knowledge.** Entries belong to the account whose
mail they were extracted from: the extraction pull reads one account,
the entries land under that account, and the draft prompt reads its own
account's entries alone. Nothing shares knowledge across accounts, by
design (the style profile's own framing).

**Extraction** is a batch job (`KNOWLEDGE_EXTRACT_VERSION`, economy
tier): the host command `knowledge_extract(account, limit)` collects the
account's most recent mail, newest first, capped at
`KNOWLEDGE_EXTRACT_LIMIT` (30) messages. The cap lives at the command
boundary: an absent limit crosses as thirty, and a named limit over
thirty crosses as thirty, because the window read would spend a body
fetch for every message the service's batch guard would then drop. Each
message rides the one batch prompt in its own untrusted wrap under a
trusted `Message N:` label, oldest first, its body capped at
`KNOWLEDGE_BODY_CAP` (2000) characters with a truncation marker. The
completion names the message each entry was drawn from, so the landing
row keeps its source; a citation outside the batch drops its entry
alone. A failed call or an unparseable completion lands nothing and
logs; the command returns the run's summary (`extracted`,
`skipped_duplicates`, `messages_considered`), whose zeros name the
failure honestly.

**The exact-text dedup guard.** The store's unique index on (account,
text) refuses a second row carrying a text the account already holds, so
a repeated extraction never stacks entries: the insert loop skips the
refusal quietly and the summary counts it. **Entries never auto-delete**;
`knowledge_forget(id)` is the only exit, on the host command and the CLI
verb alike.

**The draft prompt's knowledge context.** `knowledge_context(account)`
reads the account's entries and renders the newest
`KNOWLEDGE_CONTEXT_LIMIT` (10) as one `kind: text` line each, ordered
persona first, preferences beside them, then facts, projects, and
contacts alike by recency (the kind-relevance order). The lines ride the
drafting prompt's **trusted half as labelled data**, the A3 half of
`DRAFT_REPLY_PROMPT_VERSION` 2: model-derived text over untrusted mail
must never speak as an instruction. A failed read answers nothing, so a
draft ships without knowledge rather than failing on its shaping.

## Snippets

One row per snippet (`0008`): title, body, the store's creation and
touch stamps. Title and body are the writer's own words, never
model-derived, and ride prompts unwrapped the way style notes and rule
conditions do. The library is one store every surface shares: the CLI's
agent-first CRUD (`snippets list`, `snippets create` taking `--title`
with exactly one of `--body` or `--body-file`, `snippets delete`, each
mutating verb with `--preview`) and the composer's picker. **The replace
semantics**: a snippet is one row under its identity; an update rewrites
title and body in place and touches `updated_at`, the stamp the
recency-first listing ordering rides, so an edited snippet resurfaces
first and no edit ever grows a second row. No surface exercises
`update_snippet` yet: the store keeps it ready, and no host command or
CLI verb spells an edit.

**Ranked search** (`SNIPPET_SEARCH_VERSION`, economy tier) is ONE
structured completion over the whole library: the prompt numbers the
snippets and asks for the matched **positions** in order, best first.
Positions, not ids, ride the wire: a long id echoed imperfectly by a
model would silently cost matches, while a position is one digit the
prompt itself just named. A position outside the library or repeated in
the ranking skips alone. **The substring fallback**: a failed call or an
output no parse or repair saves falls back to the case-insensitive
substring search over titles and bodies in the library's own order; an
empty library answers without a call; a parseable ranking, an empty
match list included, is honoured as answered (an honest no-match, never
a fallback).

**The composer's picker** lists the library most recently touched first,
pulls once per mount, and crosses the search box's trimmed query to
`snippet_search` debounced at 300ms; a cleared query drops back to the
plain listing without a call. A chosen row hands its body to the insert
seam, which places it at the cursor as plain text (one line inline in
the current paragraph, further lines as paragraphs), so a body's
newlines survive and no snippet can smuggle HTML marks in.

## Autocomplete

`compose_autocomplete(before, after)` (`AUTOCOMPLETE_VERSION`, chat tier
for latency) answers one short continuation, no JSON on the latency
path: the completion trimmed, the prompt carrying the editor's text
around the cursor in two labelled halves, each in its own untrusted
wrap (the text may quote mail), the trailing half's block omitted when
the cursor sits at the end. A failed call answers an empty string and
the chip never shows.

**Gesture-only, always.** The editor surfaces the suggestion as a
dimmed chip anchored under the current line, armed two seconds after
typing stops (each keystroke restarting the window) and skipped over an
empty editor or a context unchanged since the last ask. The insert
NEVER happens without its gesture: Tab or a click on the suggestion,
both through the one insert seam the picker shares. **The dismiss** (the
chip's X) hides the chip for the rest of the session: no further asks
arm until the chip remounts with the next composition. The accept
inserts fire the editor's update, which arms a fresh idle window over
the new context (a Copilot-like loop); the dismiss stops it.

## Translate

`translate(account, key, lang)` (`TRANSLATE_VERSION`, default tier)
renders the open message's body into one of the fixed six languages
(English the default, with Chinese, German, French, Japanese, Spanish;
wire words snake_case), the body untrusted-wrapped whole and the target
language named on the trusted half. The answer renders in the pane as a
collapsible block beside the summary, labelled with its own language.

**The cache** (`0009`) is keyed (message account, message key, kind
`translate`, target language, body hash), one row per key: a new body
hash for the same language replaces the row (the one-row-per-key replace
semantics), and a different language holds its own row. A hit answers
with **no** model call and no new ledger row (the body resolve still
spends its engine fetch); the reading pane's re-expand reads it
instantly. The failure direction is a false miss, never a stale hit: a
failed or unparseable call caches nothing.

**The empty answer is the error state.** The wire answers an empty
string for both a message that no longer resolves and a call that
failed, so the block treats empty as the unnamed failure with its
sentence and the standing Translate action as the retry; a refused run
surfaces its own sentence the same way. A future wire may want the
failure named rather than emptied; recorded below as a gap.

## The never-dials rule and the CLI

The CLI reads the stores and never dials a model: `knowledge list`
(each entry with its source pair), `knowledge forget` with `--preview`,
`snippets list`, `snippets create` with `--preview`, and `snippets
delete` with `--preview`. The model-class spellings parse and refuse
(exit 3, the ledger row naming the refusal): `knowledge extract` and
`snippets search` name the rule and point at the app, the `ai style
rebuild` precedent. Autocomplete and translate have no CLI spelling at
all; they are editor and reading-pane gestures. The full verb rows and
the refusal sentences live in `docs/cli.md`.

## The eval pins and the live proof

The four prompts pin their constants through the eval harness
(`evals/`, Task 3's fixtures): `KNOWLEDGE_EXTRACT_VERSION` (two
sourced entries with their citations),
`SNIPPET_SEARCH_VERSION` (the ranked order), `AUTOCOMPLETE_VERSION`
(the continuation), `TRANSLATE_VERSION` (the cached second read), and
the draft bump to `DRAFT_REPLY_PROMPT_VERSION` 2 rides the draft
fixtures. The offline battery runs them in CI.

The gated live suite
(`client/desktop/src/ai/live_assist_tests.rs`, under `ASTRA_LIVE_LLM=1`
plus `ASTRA_LIVE_HARNESS=1` for the extraction, against the running
Stalwart harness, one test at a time) proves the services over the real
seams: a real extraction over the harness account's recent mail (the
proof's own durable note beside the seeded notes; entries landing with
their sources, the kinds inside the closed set, the summary counts
cross-checked against the store and the account's message count, the
economy-tier ledger row with sane tokens, and the sweep withdrawing the
proof's mail afterwards), a real snippet ranking over a seeded library
(a query sharing no substring with any title or body, so only the
ranked answer can be non-empty, the asked-for snippet first), a real
autocomplete continuation under the chat tier, and a real translate
answering twice on one network call, the ledger row count the
no-request proof rides.

## Matrix

| Rule | Status | Where |
|---|---|---|
| Knowledge store, five-kind closed set, exact-text dedup, forget the only exit | ✅ | `astra-ai` migration 0007 |
| Versioned batch extraction, 30-message cap at the command boundary, source-tracked entries | ✅ | `astra-app/src/ai_assist/knowledge.rs`, `client/desktop/src/commands/ai.rs` |
| Draft prompt knowledge context: ten entries, kind order, trusted half as data | ✅ | `astra-app/src/ai_assist/knowledge.rs`, `ai_replies/draft.rs` |
| Snippet store, one row per snippet, update-in-place replace, recency-first listing | ✅ | `astra-ai` migration 0008 |
| Ranked search over positions with substring fallback | ✅ | `astra-app/src/ai_assist/snippets.rs` |
| Composer picker: recent first, debounced search, insert at cursor as plain text | ✅ | `client/web/src/features/composer/SnippetPicker.tsx`, `Editor.tsx` |
| Autocomplete: chat tier, gesture-only insert, session dismiss | ✅ | `astra-app/src/ai_assist/autocomplete.rs`, `AutocompleteChip.tsx` |
| Translate: fixed six languages, one-row-per-key cache, empty as error | ✅ | `astra-app/src/ai_assist/translate.rs`, migration 0009, `TranslateBlock.tsx` |
| Host commands for all seven pulls | ✅ | `client/desktop/src/commands/ai.rs` |
| CLI reads and CRUD with `--preview`; `knowledge extract` and `snippets search` refused | ✅ | `cli/src/verbs/{knowledge,snippets}.rs` |
| Eval pins for the four prompts, offline battery | ✅ | `evals/`, `astra-app/src/evals/` |
| Gated live assist suite over the real seams | ✅ | `client/desktop/src/ai/live_assist_tests.rs` |
| The CLI runs extraction, search, autocomplete, or translate | N/A | the never-dials rule: the app runs, the CLI reads |

## Known gaps

- **The chip's Tab handling is document-scoped.** While a suggestion
  stands, Tab accepts it wherever the focus sits, which is the intent;
  a future surface with its own Tab handling inside the composer may
  want the handler scoped to the editor's own keymap.
- **The chip's unchanged-text skip is a single-slot memory.** Only the
  last asked context is remembered: returning to a context asked two
  asks ago re-asks. That is the spec's words (unchanged-text skip) and
  the cheap guard; a history would grow unbounded for no observed need.
- **The picker cannot tell a failed listing from an empty library.**
  Both answer the quiet empty panel with the same note (a failed pull
  logs and answers empty); a settings surface that manages the library
  may want the failure named.
- **The autocomplete halves cross uncapped.** The text before and after
  the cursor rides the prompt whole; a very large composition makes a
  very large ask. A cap would silently suggest from half a context; if
  live costs demand one it should be a spec decision.
- **The translate body rides whole.** A truncating cap would silently
  translate half a message, which is worse than a large prompt; the
  same spec-decision framing if costs demand it.
- **The dedup detection matches the error's text.** The extraction
  loop recognises the store's refusal by its `UNIQUE constraint
  failed` message, so a different unique violation on the knowledge
  table would skip quietly too; negligible at today's schema, a
  typed error would close it.
- **The snippet ranking prompt caps nothing.** The whole library rides
  every search; a library grown past a few hundred snippets will want a
  listing cap or a prefilter at the caller. The spec pins no cap, so
  none was invented.
