# AI digest and split inbox: the Astra Mail contract

**Scope.** The A4 catalogue features over the A1 foundation: the on-demand
digest of an account's recent mail, the analytics report over the same
window, the AI documents store both land in with their panel, the split
inbox derived from the stored categorisation with the message list's
toggle, and the host commands and CLI verbs that expose them. Meeting
briefs stay deferred (below). Binding on every surface that ships any of
it (today the desktop host, the web reading shell, and `astra-cli`).

**Principle.** *Both documents are one honest generation over mail the
app already holds; the split is no generation at all.* A digest or report
exists only because the app spent its one windowed call and stored the
answer under the window it rode; the CLI reads the store and refuses the
model-class spellings (the never-dials rule, `docs/cli.md`); the split
map is pure derivation over the incoming pass's stored rows, spending
zero model calls.

## The documents store

One row per stored document (`ai.sqlite` migration `0010`): the account,
the kind (the closed set `digest` | `report`, snake_case wire words), the
period pair the window covered (unix seconds), the content the one
generation wrote, the store's creation stamp, and the generation turn's
correlation id, so every model-derived row stays traceable to its call.
The key is (account, kind, period_start, period_end).

**Replace and stack.** A regeneration of the same period replaces the
row: the put runs its own delete plus insert inside one transaction, so
an account holds one document per kind per period, and the replacement
carries a fresh stamp and id (the answer names the replace under its
`regenerated` flag). A changed window (another day count, or the same
count asked on a later day) lands as its own row beside the old one's,
and the latest read serves the newest by stamp then id. Another account
holds the same period as its own.

## The generation window

`digest_generate(account, days?)` and `report_generate(account, days?)`
share one collection: the days default to seven, clamp to one at the
floor and thirty-one at the ceiling (the clamp repeated at the command
boundary so the webview never names a window the collection cannot
mean), and quantise to whole UTC days: the period opens at the midnight
`days` minus one days behind today's and closes at today's last second,
whatever hour the turn ran at. The quantisation is what makes the
store's same-period replace the product's regenerate.

The collection reads the account's newest `DOCUMENT_WINDOW_READ_LIMIT`
(200) rows and keeps the rows whose date falls inside the period (an
undated row never rides a window it cannot be proven in), newest first,
so the head-five each category rides are the newest five. Each kept row's
plain body is resolved through the reading path, capped at
`DOCUMENT_BODY_CAP` (2000) characters per message with a truncation
marker; a body that cannot be fetched skips its message whole, so the
rollup counts only the mail that resolved. An empty window makes no call
and stores the empty content: the panel renders an honest nothing.

## The rollup source and the Sent filter

Both documents build their category rollup from the STORED
categorisation the incoming pass recorded: each window message's newest
decided categorise word, keyed by its provider key. A message no pass
judged, or whose only categorise row was a skip, lands in the
uncategorised bucket; a foreign word already in the store rides under
its own word. Buckets answer in the closed set's order, foreign words
sorted behind, uncategorised last, every bucket's listing capped at the
newest `DOCUMENT_HEAD_LIMIT` (5) while the counts name the true totals.

**The Sent filter, recorded.** The window read crosses all mailboxes, so
the account's own Sent mail would land inside a window covering today.
The digest's prompt says what arrived; the digest's message set
therefore filters the Sent mailbox's rows out entirely (the rollup and
the listing both), the honest fix chosen over rewording the prompt: no
prompt version moved. The report keeps its own shape: its separate
computed statistic counts the window's Sent rows in code, and its
listing rides the whole window (the account's own sending is the
document's subject matter), so a failed mailbox read rides the
stats-safe zero, never a guess.

## The digest

`DIGEST_PROMPT_VERSION` 1, the digest use case, default tier: ONE
summarising call over the rollup and the grouped messages (the
inbox-zero `aiSummarizeEmailForDigest` shape, one call over the window
where inbox-zero spends one per message). The instruction, the rollup
lines, and the JSON output policy ride the trusted half; every message
rides its own untrusted wrap under its trusted category line, so a mail
body can never speak as an instruction. The completion must parse as
`{"digest": "..."}` (one repair through the shared recovery path); the
digest text is the stored content.

## The analytics report

`REPORT_PROMPT_VERSION` 1, the report use case, default tier: the
window's statistics computed in code first (the message count, the
unread rate floored to whole percent, the sent count, the per-category
counts), then ONE structured call whose output must parse as the three
pinned fields (executive summary, response patterns, recommendations),
assembled into the stored document under those three headings. The
instruction grounds the sections in the statistics and forbids invented
numbers; the statistics are the truth because no model wrote them.

## The split inbox

NO model calls: the split derives from the same stored categorisation
through `SPLIT_MAPPING` (version `SPLIT_MAPPING_VERSION` 1), pinned data
so the decision lives in exactly one place and any reassignment or new
word included is a version bump and an eval rerun:

| Stored word | Side |
|---|---|
| `primary` | `actioned` |
| `newsletter`, `notification`, `receipt`, `cold` | `fyi` |

A word the table does not name is excluded, and a message with no stored
category never enters the map: both appear in All only. The host serves
`split_map(account)` per account over the message list's own read depth
(the load window), pulled alongside every mailbox-list snapshot so the
map follows the load window at the depth the core serves; the toggle
(All / Actioned / FYI, with count badges) filters locally and never
crosses an intent, the row selection's own discipline. Thread rows name
no single message, so they render under All only.

## The panel and the CLI

The status bar's AI toggle opens the documents panel: the digest and
report tabs over each kind's latest stored document (the period label
from the window's whole UTC days, the content verbatim), a days selector
of one to thirty-one (seven selected), and Generate, which becomes
Regenerate once a document shows and refuses a second ask while one is
in flight. A refusal renders its sentence above the still-stored
document; nothing of one account ever lands in another's slot (the
panel's account epoch guard). The panel serves the selected folder's
account, the first configured account until one is.

The CLI reads the store and never dials a model: `ai digest <account>`
and `ai report <account>` answer the account's latest stored document of
their kind (the period pair, the store's stamp, the generation turn's
correlation, and the whole content), null when no generation stored one.
The model-class spellings `ai digest generate` and `ai report generate`
parse and refuse (exit 3, the ledger row naming the refusal), the `ai
style rebuild` precedent: the generation runs in the panel, and the
agent reads what landed. The full verb rows live in `docs/cli.md`.

## The eval pins and the live proof

The three constants pin through the eval harness (`evals/`):
`digest-rollup-head-five` (the rollup's bucket order, the head limit's
shown and omitted subjects, the parsed digest), `report-three-sections`
(the structured parse assembled under the headings), and
`split-mapping-pin` (the mapping table end to end). The offline battery
runs them in CI; each fixture carries its version pin.

The gated live suite
(`client/desktop/src/ai/live_docs_tests.rs`, under `ASTRA_LIVE_LLM=1`
plus `ASTRA_LIVE_HARNESS=1`, against the running Stalwart harness, one
test at a time) proves the services over the real seams: a real digest
over the seeded notes (the incoming pass's decided categorisation riding
as the rollup source; real text, the stored row under the quantised
window that covers the notes, the default-tier ledger row with sane
tokens), a real report parsing its three sections under the same honest
window, and the split map serving over the pass's real results with the
ledger row count proving no call was spent.

## Meeting briefs, deferred

The A4 spec's meeting briefs need calendar data Astra does not hold yet:
they defer to the calendar feature slice, their data dependency, and the
capabilities row stays not-started with that dependency named, never
silently dropped.

## Matrix

| Rule | Status | Where |
|---|---|---|
| Documents store, one row per (account, kind, period), same-period replace, cross-period stack | ✅ | `astra-ai` migration 0010 |
| Window: default seven, clamp one to thirty-one, whole UTC days, 200-row read cap, body fetch skip | ✅ | `astra-app/src/ai_docs/drive.rs` |
| Rollup from the stored categorisation, head-five per category, untrusted wraps | ✅ | `astra-app/src/ai_docs/{mod,digest}.rs` |
| The digest's set filters Sent; the report keeps its separate sent statistic | ✅ | `astra-app/src/ai_docs/drive.rs` |
| Digest: versioned one-call generation, default tier, strict JSON with one repair | ✅ | `astra-app/src/ai_docs/digest.rs` |
| Report: stats computed in code first, three pinned sections | ✅ | `astra-app/src/ai_docs/report.rs` |
| Split: pinned mapping, versioned, zero model calls, served per account beside the snapshot | ✅ | `astra-app/src/ai_docs/split.rs`, `client/web/src/events/surfaceEvents.ts` |
| Host commands: two generations, the latest read, the split map, days clamp at the boundary | ✅ | `client/desktop/src/commands/ai_docs.rs` |
| Panel: tabs, days selector, generate and regenerate, account epoch guard | ✅ | `client/web/src/features/ai-panel/` |
| List toggle: All / Actioned / FYI with badges, local filter, unmapped under All only | ✅ | `client/web/src/components/message-list/ListToolbar.tsx` |
| CLI reads and refused generate spellings | ✅ | `cli/src/verbs/ai.rs` |
| Eval pins for the three constants, offline battery | ✅ | `evals/`, `astra-app/src/evals/` |
| Gated live documents suite over the real seams | ✅ | `client/desktop/src/ai/live_docs_tests.rs` |
| Meeting briefs | ⬜ | deferred to the calendar slice, the data dependency |
| The CLI generates a digest or report | N/A | the never-dials rule: the panel runs, the CLI reads |

## Known gaps

- **The split map serves the load window only.** A message older than
  the list's own read depth shows in no side, matching the rows the list
  itself can show; the map refreshes with every mailbox-list pull, so
  ShowMore grows it with the window.
- **The digest's `regenerated` flag is not surfaced in the panel.** The
  store keeps the replace honest and the view answers the flag; the
  panel renders the render fields only, so no "replaced the stored
  period" word shows. One slot field away if the product wants it.
- **A filter that hides the open message resets the list selection
  only.** The reading pane stays open with its highlight rendering
  nowhere until All returns.
- **The panels stack over the same corner without mutual exclusivity**
  (outbox, agent, AI): the pre-existing behaviour, kept.
- **The report's listing includes the window's Sent rows.** The recorded
  decision (the digest filters, the report keeps its own shape); the
  sent statistic names the count beside them.
- **Two drive error branches ride unpinned offline tests**: a failed
  mailbox read's stats-safe zero (the sent count's fallback) and the
  window-read and store failures beyond the typed errors' reach. Both
  log honestly and neither surfaces half a document; a fixture that
  forces them would close the pin.
