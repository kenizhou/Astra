# AI rules: the Astra Mail contract

**Scope.** The A1 AI features on the SPA0 foundation: the rules model
(generation, matching, deterministic execution), the incoming pass that
runs them after each refresh, sender categorisation, the cold-mail
verdict, the active DLP gate, and the eval harness that pins every prompt.
Binding on every surface that ships any of it (today the desktop host and
`astra-cli`); the judge primitive itself is the binary judge spec's
contract, referenced here, not restated.

**Principle.** *Models judge and draft; the deterministic core acts.* A
model may answer one bit (a match, a verdict) or one structured value (a
category, a rule), never take an action directly. Actions run through the
app's own write paths, audited, under the pass's one correlation id, and
an uncertain model always resolves to the safe default the table names.

## The rules model

A rule is four fields: a short name, one natural language condition
sentence, a closed list of actions, and an enabled flag. Rules live in
`ai.sqlite` (migration `0002`), the one store the ledgers, the results,
and the CLI share.

The action set is closed: `mark_read` and `set_flagged`, the
deterministic, reversible pair A1 ships. Unknown action words are
rejected loudly everywhere a rule crosses a boundary: a generation that
names one fails whole, a stored payload that carries one fails its load,
and the CLI's verbs refuse it at parse. The wider action set (folders,
draft replies) arrives with its slice and grows the closed set
additively, with the pins that tie every spelling moved in the same
change.

**The closed set is spelled twice, by design.** The app layer's
`Action::ALL` and the CLI's `CLOSED_ACTIONS` carry the same wire words in
two crates that do not depend on each other (the CLI depends on
`astra-ai` alone, not `astra-app`). Both are pinned by their own tests; a
change to either word lands in both places with their tests in the same
change. This duplication is the honest cost of the CLI's dependency
floor, not an oversight.

### Generation

`prompt_to_rules(text)` converts plain English instructions into rules:
one versioned prompt (`RULES_PROMPT_VERSION`), one completion through the
default tier, one strict parse with a single `repair_json` recovery when
the model wrapped its JSON in prose. The instruction text is user-authored
and rides unwrapped (trusted, the generation prompt's own contract); mail
content never rides this prompt at all. Generated rules arrive enabled
with freshly minted ids; storing is the caller's step.

**The CLI generation seam.** `astra-cli rules create` takes an explicit
condition sentence and generates nothing: the CLI holds no model client
and never will (the offline-provider contract; it dials no server of any
kind). The agent that drives the CLI composes the condition itself,
through its own model, and the verb stores what it is given. Generation
through the background client lives in the desktop host, where the client
is assembled at boot.

### Matching

One rule per message is one `JudgeRuleMatch` call at the economy tier:
the condition sentence rides the trusted question half as data, and the
mail (sender, subject, body capped at `MATCH_CONTEXT_BODY_CAP` characters
with a truncation marker) rides the context half the host wraps with the
hardening fence, so mail can never speak as an instruction nor a
condition as mail. Questions are phrased so `Yes` is the positive pole
(the match); an abstaining judge resolves to **no match** and the rule
does not run. Nothing executes on the safe-default table's answer:
deterministic actions need positive matches.

### Execution

A `Yes` verdict runs the rule's actions through the app's existing write
paths (mark read, set flagged), audited under the pass's correlation id,
each action idempotent and reversible exactly as the surface's own menu
is. The model never executes anything; it only answered whether the
condition held.

## The incoming pass

After each refresh's rebuild the app arms one bounded pass; a driver
future the host runs (the same seam the drafts autosave driver rides)
picks it up off the refresh's own path, so model latency never sits on
the refresh the user asked for. One pass mints one correlation id, reads
the enabled rules, and judges the head of the newest unread messages no
earlier pass has judged (at most `PASS_MESSAGE_LIMIT` per pass, drawn
from a newest-first candidate window; seen tracking is durable through
the results rows themselves, so a restart rejudges nothing).

Each candidate passes the DLP gate first, then runs its stages in order:
the rule matches (one judge call per enabled rule), categorisation, and
the cold judge. Every verdict lands in migration `0003` (`ai_results`:
message account and key, stage, rule id where the stage judged one,
verdict word, category) under the pass's correlation id, and every model
attempt lands its `llm_calls` row beside it. Failures log and the pass
moves on; nothing the pass does may fail the refresh that armed it. The
pass runs only when the host assembled the AI bundle (both seams over the
background client, the DLP policy, the one store); without one, the arm
is a no-op and the driver sleeps on.

## Categorisation

One structured completion per message (default tier, versioned under
`CATEGORISE_PROMPT_VERSION`) sorts it into exactly one of five closed
words: `primary` (a person's own mail, written by hand), `newsletter`
(a recurring bulk mailing opted into or opt-out-able), `receipt` (a
transaction record, invoice, or shipping notice), `notification` (an
automated service notice), `cold` (unsolicited outreach, selling
something). The taxonomy is closed the way the action set is: a
completion naming anything else fails its categorisation and the stage
records itself `skipped`, never guessed. The category lands in the
results row; no A1 surface consumes it yet (the badges arrive with A2).

## The cold verdict

One `JudgeColdEmail` judge call per message (nano tier): is this
unsolicited cold outreach from a sender with no prior relationship? The
question is phrased so `Yes` is the cold pole, and the safe default is
**No, not cold**: an uncertain judge leaves the mail exactly where it
is. A1 executes nothing on the verdict; it is recorded (the results
row's verdict word, `abstain_default` when the table answered) and read
back by `astra-cli ai cold`. Moving mail on a coin flip is worse than
leaving it; the action that cold mail might earn arrives with a later
slice and its own contract.

## The safe defaults, referenced

Every judge use case declares its abstain default up front in
`SAFE_DEFAULTS` (`astra-ai`), pinned by tests that fail compilation when
a use case lacks a row. The table, its rationale, and the boundary
(judges gate user experience only, never authorisation; the permission
bridge and send refusal stay deterministic) are the binary judge spec's
contract: `docs/superpowers/specs/2026-10-10-binary-judge-design.md`.
A1's rows: cold -> not cold, rule match -> no match, and the eval judges
-> fail. The spend guard (`ASTRA_LLM_NANO_BUDGET_MICROS`, checked before
every judge call against the ledger's trailing-week nano aggregate;
on exceed the safe default answers with no network call and one
`budget-skipped` row) is likewise that spec's and ships as it pins.

## DLP: the gate in front of every mail-carrying prompt

`ASTRA_DLP` selects one posture for the whole pass, read once at boot:
`allow` (the default) passes content through untouched, `redact` masks
sensitive spans before they ride any prompt, `block` refuses the
message's AI stages outright (one `dlp` results row, verdict `blocked`,
no seam call spent). The gate applies per message, per field (sender
name, sender address, subject, body), before any stage prompt is built.

A malformed value fails the pass's arming loudly at boot (an error log
naming the variable and the accepted words, and no AI bundle assembled):
a misconfigured policy is never silently treated as allow.

`redact` masks email addresses, phone-shaped digit runs (digits joined
by separator gaps, 7 to 15 digits), and long digit runs (twelve digits
or more, separators or none), one mask character per character so layout
survives. The heuristics are honest about their corners, and the eval
fixtures under `evals/` pin the boundary, including the two documented
unmasked corners: an address immediately followed by CJK full-width
punctuation glues the next word into the TLD and stays unmasked, and an
address whose TLD carries non-ASCII bytes fails the alphabetic check and
stays unmasked. Both are recorded as corners, not fixed; the masking net
widens only with fixtures that pin the widened shape.

## The eval harness and the version pins

Scenario fixtures live under `evals/` (one JSON file per scenario:
feature, mode, the pipeline constant value its prompt text was written
against, inputs, expected outcome). Offline scenarios run as ordinary
tests in CI over injected fixture answers; live scenarios (judge-backed,
model-answer-dependent) run only under `ASTRA_LIVE_LLM=1`, driven by the
desktop host's live suite through the same seam-parameter drive. The
runner and fixtures live beside the seams they exercise, so an eval
failure names the contract that moved.

Every prompt-carrying scenario pins the pipeline constant it was written
against (`RULES_PROMPT_VERSION`, `CATEGORISE_PROMPT_VERSION`,
`COLD_JUDGE_VERSION`, `RULES_MATCH_VERSION`, `JUDGE_TEMPLATES_VERSION`),
and the offline battery asserts each pin equals the build's value: a
prompt change that bumps a constant without rerunning and refreshing its
scenarios fails the offline suite. The AI rule this enforces: an AI
behaviour change requires an eval run in the same change and a pipeline
version bump.

**`JUDGE_TEMPLATES_VERSION` in the ledger: ruled out for now.** Carrying
the prompt version inside each `llm_calls` row needs a schema change to
the ledger and every writer (host client, sidecar relay, CLI) moved in
one change; the version-pin net already makes a silent template bump
fail offline, and the constant ships beside the template it versions.
The ruling stands recorded here as a known gap; the column arrives only
if a debugging need ever demands per-row prompt provenance.

## The live proof

The gated live suite (`client/desktop/src/ai/live_tests.rs`, run under
`ASTRA_LIVE_LLM=1` with the connection environment and, for the harness
tests, `ASTRA_LIVE_HARNESS=1` against the running Stalwart harness)
proves the stages over the real seams: generation parses, categorisation
decides inside the closed set, a real judge call lands a verdict for a
genuinely seeded harness note in `0003` and the ledger, token counts and
outcomes land in `llm_calls` (sane bounds, never exact pins), the
live-mode eval scenarios hold against model answers, and one whole
refresh drives the pass end to end with every stage row under one
correlation id.

**One endpoint quirk, seen and recorded.** The verified endpoint serves
a reasoning model behind the anthropic-compatible wire: its `content`
array carries `thinking` blocks before the text block (the parser takes
the first text block, so the thinking rides harmless), and a response
whose thinking consumed the whole `max_tokens` budget arrives with no
text block at all. The client classifies that textless 2xx exactly as
it classifies any malformed envelope: retryable, advancing the chain;
a judge folds it into the safe default; a single-entry chain surfaces
it as unreachable. No special case exists for it, by design: the
classification, not the endpoint's name, is the contract.

## Matrix

| Rule | Status | Where |
|---|---|---|
| Rules store with the closed action set, loud rejections | ✅ | `astra-ai` migration 0002 |
| Versioned generation prompt with one repair | ✅ | `astra-app/src/ai_rules/generate.rs` |
| Matching: wrapped context, positive-pole questions, abstain means no match | ✅ | `astra-app/src/ai_rules/match.rs` |
| Execution only on a model's Yes, through the app's write paths | ✅ | `astra-app/src/ai_rules/execute.rs` |
| Bounded incoming pass, durable seen tracking, one correlation per pass | ✅ | `astra-app/src/ai_results.rs` |
| Host wiring: both seams over the background client, boot DLP read | ✅ | `client/desktop/src/ai/seams.rs`, `boot.rs` |
| Categorisation: five-way closed set, skipped never guessed | ✅ | `astra-app/src/ai_rules/categorise.rs` |
| Cold verdict recorded, nothing executed on it in A1 | ✅ | `astra-app/src/ai_rules/cold.rs` |
| DLP gate: allow, redact, block; malformed fails the arming loudly | ✅ | `astra-ai/src/dlp.rs`, `client/desktop/src/ai/seams.rs` |
| Spend guard before every judge call | ✅ | `client/desktop/src/ai/client/judge_call.rs` |
| Eval harness: offline battery, live gate, version pins | ✅ | `evals/`, `astra-app/src/evals/` |
| Gated live suite over the real seams | ✅ | `client/desktop/src/ai/live_tests.rs` |
| CLI: rules CRUD and stored-verdict reads | ✅ | `cli/src/verbs/{rules,ai}.rs` |
| The CLI generates rules itself | N/A | the seam above: the agent composes, the CLI stores |
| Badges or surfaces over the verdicts | ⬜ | A2 |
| Cold verdict moves or archives mail | ⬜ | a later slice, its own contract |

## Known gaps

- **Html-only mail rides the stages with an empty body.** The pass
  builds its contexts from the engine's plain-text body; a message whose
  body is html only (the harness's rich note, say) carries its subject
  and sender into the prompts but no body text. The reading surface sees
  the html; the AI stages do not. A plain-text extraction at the pass is
  the fix when a stage needs it, landed with its own fixtures.
- **Single global correlation holder.** The pass's correlation spans its
  own rows cleanly, but the agent relay's single global holder and the
  `any_correlation` attribution of session-less asks remain
  single-session constraints (the AI foundation's recorded gap); the two
  worlds meet only in the one ledger file.
- **`JUDGE_TEMPLATES_VERSION` is not a ledger column.** Ruled out above;
  recorded here so the gap is found with its ruling.
- **The redaction corners.** The two unmasked corner fixtures under
  `evals/` are documentation, not fixes; see the DLP section.
- **Live scenarios are a small set.** Three live fixtures ship (cold,
  match, categorise); the offline battery carries the shape coverage.
  Growing the live set is an eval-content change, not a contract one.
- **The CLI's closed-set duplication.** Spelled twice by design; the
  sentence lives in the rules model section above.
