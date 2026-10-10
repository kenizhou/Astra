# Binary Judge Design

Date: 2026-10-10
Status: proposed (awaiting review)
Parent spec: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (sections 8, 9)
Feeds: A1 (rules, categorisation, cold blocker, eval harness), A2 (draft surfacing), A3 (knowledge gating), A4 (digest selection)

## 1. Purpose

A large share of Astra's AI decisions are one bit: is this cold mail, does
this rule match, is this draft worth surfacing, is this content safe to
send to the model, did this behaviour pass the eval. These decisions want
a dedicated primitive: a binary judge call that is cheap, strict, and
honest about uncertainty, rather than prose completions parsed after the
fact.

## 2. The primitive

- **`judge(use_case, question, context, correlation) -> JudgeVerdict`**
  on the background client (Rust) and, where the sidecar needs it, a
  mirrored tool: one prompt built from a versioned judge template
  (question + tagged context, untrusted content wrapped by the
  hardening wrapper), one completion forced to a single token, parsed to
  `JudgeVerdict::{Yes, No, Abstain}`.
- **Abstain is a first-class state.** A judge that is unsure says so.
  Every consumer declares its safe default for Abstain up front (the
  safe-default table below); retry-on-abstain happens once at the same
  tier with a colder prompt, then the default applies. Abstain is never
  silently collapsed into Yes or No.
- **Parsing is strict**: `yes`/`no` (case-insensitive, after trimming
  punctuation) or the Abstain marker; anything else is Abstain. No JSON
  envelope, no repair loop: the single-token form is the whole point.
- **Ledger**: every judge call is an `llm_calls` row like any other
  (use case, tier, tokens, correlation id); the verdict is recorded in
  the row's outcome field so the judge population is auditable.

## 3. The safe-default table

| Use case | Tier | Abstain default | Rationale |
|---|---|---|---|
| `JudgeColdEmail` | nano | No (do not move) | moving mail on a coin flip is worse than leaving it |
| `JudgeRuleMatch` | economy | No (rule does not run) | deterministic actions need positive matches |
| `JudgeDlp` | nano | Block (fail closed) | privacy errs shut |
| `JudgeInjection` | nano | Treat as hostile (wrap harder) | security errs closed |
| `JudgeDraftSurface` | nano | No (do not surface) | unsurfaced drafts still exist in the folder |
| `JudgeSummaryFaithful` (eval) | economy | Fail the eval | evals err strict |
| `JudgeScenario` (eval) | economy | Fail the scenario | same |

## 4. The boundary: judges never gate security

The permission bridge, the send-class refusal, and the CLI gating stay
deterministic. A judge verdict may gate user experience (surface, move,
wrap, select) never authorisation. A judge saying Yes to "is this tool
call safe" changes nothing; the policy table and the human decide. This
boundary goes in AGENTS.md beside the security adjudication rule.

## 5. Use-case inventory (registry growth)

SPA0 shipped six use cases; the judge family joins as features land:
A1 adds JudgeColdEmail, JudgeRuleMatch, JudgeInjection, plus the
eval-harness judges (JudgeScenario, JudgeSummaryFaithful); A2 adds
JudgeDraftSurface; A3 and A4 add their selection judges as designed.
All map to nano or economy tiers; none to chat or draft.

## 6. Eval harness fit (A1)

The eval harness inbox-zero patterns suggest lands with A1, and judges
are half of it: scenario transcripts verdicted by JudgeScenario
(deterministic fixtures where possible, judge where the behaviour is
linguistic), with judge prompts versioned under the pipeline constants
and judge calls ledgered like production. Deterministic-fixture-first:
a scenario that can be asserted without a judge must be.

## 7. Cost shape

Single-token outputs make judges the cheapest call class: the nano tier
exists for them, the weekly spend guard (A1) watches nano first. Judge
templates are short and versioned; context is capped per template
(head-N truncation with a marker, never whole mailboxes).

## 8. Testing

- Unit: the strict parser (yes/no/abstain/punctuation/case/garbage); the
  retry-once-then-default state machine; the safe-default table itself
  as a pinned test so a new use case cannot ship without a default.
- Fixture servers: judge wire shape (the single-token prompt form) and
  the abstain path.
- Evals: judge prompt changes bump the pipeline constant and rerun the
  scenario suite (the rulebook's AI rule).
