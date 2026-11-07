# SP2y plan: the assistant chat with a tool budget

Spec: docs/superpowers/specs/2026-11-07-sp2y-assistant-chat-design.md. Branch
`sp2y-assistant-chat` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit.

### Task 1: the tool budget (C1)

**Files:** sidecar/src (the prompt runner's clock, the gated tool runner's deadline
view, the watchdog, the budget-spent state event); client/web/src/features/agent
(the status line and the bound note); client/desktop/src/agent (the event folding);
the mock harness; evals/ (the budget scenarios).

- [ ] Tests first; commit `feat: bound the assistant's tools by the clock`.

### Task 2: the search and act tools (C2)

**Files:** cli/src/verbs/messages.rs (mark-read, set-flagged, archive, trash,
search; each with --json and --preview); sidecar/src/tools (the new tools,
confirm-gated, the truncation caps); the runtime adapter's allowlist and the skill
index wording; the desktop host's policy rows.

- [ ] Tests first; commit `feat: give the assistant the search and act tools`.

### Task 3: persistence and threads (C3)

**Files:** crates/astra-ai/src/store.rs (the chat migration, the rows, the reads);
client/desktop/src/agent (the per-chat session ids in the relay's messaging, the
load, list, rename, delete commands); client/web/src/features/agent (the thread
list, the store as the one source of truth).

- [ ] Tests first; commit `feat: keep the assistant's threads`.

### Task 4: the cards, the markdown, the version, the eval (C4, C5)

**Files:** client/web/src/features/agent (the structured tool cards, the markdown
render, the dependency judged); the run metadata stamp; evals/ plus the pipeline
version bump; docs (the AI rules' pipeline pin section, the chat's doc) + zh.

- [ ] Tests first; commit `feat: show the assistant's tools and stamp the run`.

### Task 5: the live proof + the row + the fragment

**Files:** the gated live proof over the harness (a turn that searches the seeded
mail, acts through the confirm gate, and loses its tools at an armed short budget,
landing done); the capabilities row + zh; the changelog fragment.

- [ ] Commit `feat: prove the chat live and record it`.

### Task 6: close-out

- [ ] Full battery; branch pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: the budget (T1), the tools (T2), the threads (T3), the cards and
  the version (T4), the proof and the row (T5).
- Risks: the done-envelope guarantee must hold on the watchdog path (a cap abort
  lands done, never hung); a past-budget tool call lands its audit row (never a
  silent drop); the two persistence stores must not both paint (the store is the
  one truth); the markdown dependency is new (sandboxed, no raw HTML); the send
  refusal stays structural (no verb, no tool, the skill index says human).
