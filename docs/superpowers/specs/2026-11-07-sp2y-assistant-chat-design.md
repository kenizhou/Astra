# SP2y design: the assistant chat with a tool budget

**Status.** Decided 2026-11-07. Slices branch from main at this commit.

**Scope.** The A2 row: an assistant chat whose turns may call tools, bounded by a
wall-clock tool budget. The reference is inbox-zero (the `ai-design` lock in
`refs/refs-lock.toml`), not mail-calendar, which holds no chat surface; the survey
lives in the slice's ledger.

**Licence posture, stated up front.** inbox-zero is AGPL-3.0 with added commercial
terms (its licence adds a monetisation restriction and an enterprise use limitation).
The posture this slice takes is the established one (the use-case table port in
`ai-foundation.md`): port the CONTRACT and the BEHAVIOUR with attribution naming the
source file and repository, and re-express the code in Astra's own idiom under
GPL-3.0-only. No inbox-zero file is copied; no allodia-licensed code is touched (the
chat path imports nothing from mail-calendar at all).

What already exists and is reused unchanged: the sidecar's JSONL stdio loop and its
done-envelope guarantee, the streaming events across the host boundary, the
permission bridge (fail closed, an absent row means needs-approval), the ledger
(every LLM call, tool call, and permission decision with a correlation id), the tier
registry's AssistantChat resolution, the abort semantics, and the one streaming chat
panel. What this slice adds: the budget, the search and act tools with their CLI
verbs, chat persistence and threads, structured tool surfacing with markdown, and
the pipeline version stamp with its eval.

## C1: the tool budget

The reference's budget is wall-clock from turn start; past it the model loses its
tools (the active set emptied, tool choice none) and finishes in text; nothing in
its UI says so. Astra's bound is coarser by necessity (the agent runtime has no
per-step hook), and this spec says so honestly:

- The clock starts at prompt receipt (the prompt runner's turn).
- A tool execute past the bound answers the model the SKIPPED outcome the harness
  already reports (never a silent drop; the audit row lands as it does for a
  refused tool), so the model learns the budget the only way it can: by being told
  no.
- A watchdog aborts the turn at a hard cap (the budget plus a grace), landing as a
  DONE envelope with a state event, never a hung prompt: the done-envelope
  guarantee holds on every path. A text-only run has no tool gate at all, which is
  why the watchdog is not optional.
- A `budget-spent` state event crosses to the panel (the reference's silence is not
  a contract to copy): a status line while a turn runs, and a note when the bound
  bites.
- The mock harness gates the same way or the end-to-end proof proves nothing.

## C2: the search and act tools

- The CLI gains the mail action verbs (messages mark-read, set-flagged, archive,
  trash; each with `--json` and `--preview` per the CLI rules) beside the read-only
  family, plus the messages search verb (the MCP surface's search shape is the
  contract; the app layer's verbs already exist).
- The sidecar gains the tools beside the five mail tools, confirm-gated through the
  policy table (reads auto, actions confirm, everything else fails closed), the
  runtime allowlist extended, and the skill index wording updated to name what the
  assistant may now do and that sending stays human.
- The search tool pages (twenty per call) with a has-more answer, the reference's
  own pagination discipline; tool outputs are strings on the wire, so the
  truncation policy adopts the reference's caps (the read's four-thousand-character
  bound, the log's two-hundred).
- No send tool exists anywhere (the CLI has no send verb, the MCP surface has no
  send tool, the sidecar refuses send-class verbs to the agent): the structural
  refusal is the contract, unchanged.

## C3: persistence and threads

- Chat and message rows land in the AI store as a new migration: the chat's id,
  title, created and last-touched instants, and the messages' role, parts (the
  rendered turns), and run metadata.
- Every prompt rides its own session id (per chat, not the fixed main session),
  routed by the session id the sidecar already reads.
- The panel gains the thread list with load, list, rename, and delete commands.
- ONE source of truth for the visible log: the store's rows. The runtime's own
  session files under the agent data directory stay engine-room, never what the UI
  loads; the two can disagree and only the store's answer paints.
- The earlier trim's "no conversation mirror" is reopened for exactly this mirror;
  compaction and memory extraction STAY OUT (recorded as the row's named remainder,
  not silently dropped).

## C4: tool surfacing and markdown

- The note lines become structured tool cards (name, argument summary, status,
  truncated output).
- Assistant text renders as markdown; the dependency is judged at implementation (a
  small renderer, sandboxed, no raw HTML; the composer security document's
  sanitiser discipline applies to what renders).

## C5: the pipeline version and the eval

- Every run stamps its metadata with the pipeline version (the chat rows' metadata
  field; the ledger itself gains no column this slice).
- An AI behaviour change ships with its eval: scenarios land under `evals/` pinning
  the budget's behaviour (a past-budget tool call answers skipped; the watchdog
  lands done; the budget event crosses), and the version bumps in the same change.

## Known gaps recorded, not hidden

- Compaction and memory extraction stay out (the reference's chat compaction and
  chat memory have no Astra counterpart this slice).
- Inline email cards stay out: the draft card is the pending action (the draft tool
  already ships), and the reference's inline-card prompt discipline does not port
  to a product whose reading pane already renders messages.
- The budget is wall-clock only; no per-turn token or step cap (the reference's
  step count is unbounded by the same choice).
- The chat is single-user local; no resumable streams across restarts (the
  reference's resume is a server shape Astra does not have).

## Test-first

Every behaviour lands with its red: the budget's tool gate and watchdog over the
mock harness (skipped outcomes, audit rows, the done envelope, the state event);
the verbs' `--json` and `--preview` pins; the tools' confirm gating and the
truncation caps; the migration's rows and the per-chat session routing; the cards
and the markdown render; the version stamp and the eval scenarios; and a live
proof over the harness: a chat turn that searches the seeded mail, acts on a row
through the confirm gate, and loses its tools at an armed short budget with the
turn still landing done.
