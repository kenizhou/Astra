# SPA1/A1 AI Rules, Categorisation, Cold Blocker Design

Date: 2026-10-10
Status: proposed
Parent specs: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (section 9.3), `docs/superpowers/specs/2026-10-10-binary-judge-design.md`
Consumes: the verified live endpoint (tier 3 testing) and the SPA0 registry, ledgers, client, CLI, sidecar.

## 1. Purpose

The first AI features on the SPA0 skeleton: plain-English rules generated
into structured conditions, incoming mail judged against them with
deterministic execution, sender categorisation, a cold-mail judge, the
binary judge primitive with its safe defaults, active DLP enforcement on
every mail-content prompt, the eval harness with its judges, and the
nano spend guard.

## 2. Decisions

- **D1 (judge primitive)**: per the binary judge spec exactly:
  `LlmClient::judge(use_case, question, context, correlation) ->
  JudgeOutcome` where JudgeOutcome is the verdict plus which path produced
  it (first try, retry, abstain default); strict single-token parsing;
  retry once on Abstain at the same tier; the safe-default table lives in
  astra-ai as data and is pinned by a test that fails when a judge use
  case lacks a default.
- **D2 (rules)**: rules live in the product store (migration `0002`:
  id, name, condition (natural language, versioned prompt template),
  actions (a closed set: `mark_read`, `set_flagged`; the wider action set
  arrives with the folders and draft-reply slices, recorded), enabled).
  `prompt_to_rules(text)` generates structured rule JSON through
  `complete` + `repair_json` (DEFAULT tier, pipeline constant
  `RULES_PROMPT_VERSION`); incoming mail matching runs JudgeRuleMatch
  per rule (economy tier, context capped); execution is deterministic
  through the existing astra-app write paths; every run is ledgered and
  audited with the turn's correlation id.
- **D3 (incoming pass)**: after each refresh's rebuild, a bounded pass
  (head N of newly seen unread messages, N small and constant) runs the
  AI stages: rule matches, categorisation, cold judgment; results land
  in migration `0003` (message key, stage, verdict, category) keyed by
  correlation id. No snapshot or UI change in A1 (badges arrive with
  A2's surfaces); results are queryable via the CLI.
- **D4 (categorisation + cold + DLP)**: categorisation is a structured
  one-of-five call (primary, newsletter, receipt, notification, cold;
  DEFAULT tier, versioned); cold is the JudgeColdEmail binary judge
  (nano). Every prompt carrying mail content wraps it with the
  hardening wrapper; DLP graduates from allow-only: `redact` masks
  emails, phone-shaped numbers, and long digit runs before wrapping;
  `block` refuses the AI pass for that message (ledger outcome
  `blocked`); the policy comes from `ASTRA_DLP` (default allow).
- **D5 (eval harness)**: `evals/` holds scenario fixtures (prompt,
  inputs, expected verdicts); a cargo-test runner executes deterministic
  scenarios offline (no network: fixture completions) and judge-backed
  scenarios only under `ASTRA_LIVE_LLM=1`; judge templates and feature
  prompts carry pipeline constants (bumped with any prompt change, the
  eval rerun demonstrated in this slice's PR).
- **D6 (CLI growth)**: `rules list|create|delete` and `ai cold
  <message-key>` land with `--json`, audit, and `--preview` on
  mutating verbs, per the CLI contract.
- **D7 (nano spend guard)**: the weekly nano budget
  (`ASTRA_LLM_NANO_BUDGET_MICROS`, generous default) is checked from the
  ledger's weekly aggregate before each judge call; on exceed the pass
  skips with a warn (fail open for UX stages; recorded honestly, DLP
  judge skip means allow under the default policy).

## 3. Testing

- Unit: the judge parser and retry state machine; the safe-default
  table pin; redact patterns; rule execution determinism (fakes record
  the write intents); the bounded incoming pass (N honoured, no re-run
  on seen messages).
- Fixture servers: judge wire (single-token prompt form), abstain path,
  structured generation shape incl. the repair path.
- Evals: deterministic scenarios offline in CI; judge scenarios under
  the live gate.
- Live (tier 3, gated, the provided endpoint): prompt-to-rules on a
  sample sentence returns parseable rules; a real judge call on a seeded
  harness mail returns a verdict; costs land in the ledger; the whole
  A1 suite green against the Docker harness.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build, vitest,
   sidecar tests green.
2. Offline: rules CRUD via the CLI round trips; a rule matches a fake
   incoming mail and executes deterministically; redact and block
   paths pinned; the eval runner passes deterministic scenarios in CI.
3. Live: the gated suite passes against the provided endpoint and the
   Docker harness (generation, matching, cold judge, ledger costs).
4. A prompt change bumps its pipeline constant and the eval rerun is
   demonstrated (one deliberate bump in this slice's PR).
5. `docs/ai-rules.md` (+ zh) with the contracts (rules, categories,
   cold, the judge safe-default table reference, the spend guard, the
   eval harness), capabilities rows moved with mirrors, fragments.
