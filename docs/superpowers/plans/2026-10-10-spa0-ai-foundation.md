# SPA0 AI Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand the AI skeleton: the astra-ai registry and ledgers (product SQLite arrives), the background dual-protocol LLM client with tier fallback, the astra-cli agent tool surface with audit, the kylins-pattern sidecar (bundle, protocol, safeExec, permission bridge, tools), the host relay/supervisor/panel, and the offline e2e conversation with end-to-end correlation ids.

**Architecture:** astra-ai owns types, the tier table, and the sqlx store (llm_calls + audit_log) shared by host and CLI; client/desktop runs the background client and the sidecar relay; sidecar/ is the esbuild CJS package on the pi SDK; cli/ is the agent-first binary over the engine store.

**Tech Stack:** Rust (sqlx sqlite joins the workspace table), Node >= 22 sidecar (pi SDK at kylins' pin, esbuild), reqwest for the client, existing harness patterns.

**Spec:** `docs/superpowers/specs/2026-10-10-spa0-ai-foundation-design.md`

## Global Constraints

- House rules as prior slices: SPDX, 500-line cap, deny-everything lints, gate before commit (direct exit, never piped), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- Port roots: kylins = D:/Projects/mailclient/kylins (kylins.client.agent-sidecar, kylins.client.backend/src/agent, kylins-cli); inbox-zero = D:/Projects/mailclient/opensource/inbox-zero (apps/web/utils/llms for the tier/fallback shapes); engine via `git show 68eab00:<path>`.
- Security floor (parent spec section 8): keys in memory only; safeExec scrubbed env + prefix allowlist; permission bridge fail-closed; send-class verbs refused; correlation ids on every ledger and audit row.
- AR1-AR7 apply (kylins mailcal renames: kylins -> astra; no history comments).

---

### Task 1: astra-ai crate (registry, ledgers, hardening types)

**Files:**
- Create: `crates/astra-ai/src/{lib.rs, use_case.rs, tier.rs, tier_tests.rs, store.rs, store_tests.rs, migrations/0001_init.sql, correlation.rs, hardening.rs, hardening_tests.rs}` + Cargo.toml
- Port shapes from: inbox-zero `apps/web/utils/llms/{model.ts, use-cases.ts}` (tier list syntax, use-case naming); kylins' llm_calls columns + audit shape.

**Interfaces:**
- Produces: `LlmUseCase` (six SPA0 variants, serde snake_case); `ModelTier::{Default, Economy, Chat, Draft, Nano}` (wire words default/economy/chat/draft/nano); `TierTable::from_env()` resolving a use case to an ordered provider:model chain (env vars `ASTRA_LLM_DEFAULT` etc., comma lists, first-primary-rest-fallback; parse errors are loud); `AiStore::open(path)` (sqlx sqlite, runs embedded migrations) with `record_llm_call(LlmCallRow)` and `record_audit(AuditRow)`; `LlmCallRow { correlation: CorrelationId, use_case, tier, model, prompt_tokens, completion_tokens, cost_micros: Option<u64>, latency_ms, outcome }`; `AuditRow { correlation, verb, arg_preview, outcome }`; `CorrelationId` newtype (serde, generated via `CorrelationId::generate()`); `tag_untrusted(heading, content) -> String` (the <untrusted> wrapper) + `DlpPolicy::{Allow, Redact, Block}` with `resolve(policy)` returning Allow in SPA0 (redact/block land with A1, recorded in the type docs).

- [ ] Step 1: tier_tests first (env parsing: single, chain, malformed loud; per-tier defaults; use-case to tier mapping for the six).
- [ ] Step 2: store_tests first (open in-memory, migrations apply, llm_call and audit round trips preserve every field; correlation id round trips as the same string).
- [ ] Step 3: hardening_tests first (wrapper wraps with heading and fence; idempotence on already-wrapped content).
- [ ] Step 4: Implement; sqlx joins the workspace table (sqlite, runtime-tokio); crate is member, default-members included; all green; commit `feat: add the ai registry, ledgers, and hardening types`.

### Task 2: background LLM client (dual protocol, fallback, ledger)

**Files:**
- Create: `client/desktop/src/ai/{mod.rs, client.rs, client_tests.rs, wire.rs}` (+ fixture server helper under test)
- Port shapes from: kylins `kylins.client.backend/src/agent/llm_client.rs` (dual-protocol wire) + inbox-zero `utils/llms/index.ts` (fallback-on classes).

**Interfaces:**
- Produces: `LlmClient::new(http, registry: TierTable, store: Arc<AiStore>)`; `async fn complete(&self, use_case, prompt, correlation) -> Result<String, LlmError>` (non-streaming completion; the streaming variant arrives with A2's chat UI, recorded); protocol selected per model entry (openai:/anthropic: prefix in the tier entry; kylins' convention; read its config spelling and mirror); retryable errors (connect, 5xx, 429-with-retry-after) advance the fallback chain; quota and auth errors are terminal for the call; every attempt writes an LlmCallRow; JSON repair helper `repair_json(raw)` for structured outputs (balanced-brace extraction; the caller retries once with the repair).

- [ ] Step 1: client_tests first with a local one-shot HTTP fixture (the autodetect pattern): success per protocol (assert both wire shapes byte-exactly from the fixture's capture); retryable failure on the primary then success on the fallback (two ledger rows, tier advanced); quota terminal (one row, typed error); malformed JSON repaired.
- [ ] Step 2: Implement; keys read once at startup from `ASTRA_LLM_API_KEY_<PROVIDER>` style env (mirror kylins' naming; record) and held in memory.
- [ ] Step 3: All green; commit `feat: add the background llm client with tier fallback and ledger`.

### Task 3: astra-cli (agent-first verbs, envelope, preview, audit)

**Files:**
- Create: `cli/src/{main.rs, envelope.rs, envelope_tests.rs, verbs/{mod.rs, accounts.rs, folders.rs, messages.rs, drafts.rs}, audit.rs}` + per-verb tests; modify `.github/workflows/ci.yml` (build the CLI in the desktop job), `client/desktop/tauri.conf.json` (externalBin staged; beforeBuildCommand builds the CLI; read kylins' staging script mechanism and mirror minimally with a scripts/stage-cli.mjs)

**Interfaces:**
- Produces: bin `astra-cli` with the six verbs; `--json` envelope `{schema: 1, ok: true, data}` / `{schema: 1, ok: false, error: {code, message}}` (schema pinned); `drafts save` and `drafts discard` ship `--preview` (the preview prints the action the engine call will take, single source: the same builder the verb uses); every invocation opens `AiStore` beside the data dir and audits (verb, redacted arg preview, ok/err, correlation id from `--correlation-id` or generated); reads open the engine store at `ASTRA_DATA_DIR` (read-only opens; record the second-writer seam honestly: draft writes are the engine's own put/delete under its locking; the app is notified by its sync paths on next refresh; record as the known interaction); exit codes: 0 ok, 1 runtime error, 2 usage; send-class verbs (`send`, `queue`) are parsed-but-refused with exit 3 and a message naming the human-send rule.

- [ ] Step 1: envelope_tests first (both shapes byte-pinned; schema field constant).
- [ ] Step 2: per-verb tests first over an in-memory engine (list/read shapes; drafts save/discard round trip incl. preview equality; audit rows per invocation with correlation id).
- [ ] Step 3: Implement + CI/staging wiring; the desktop beforeBuildCommand change verified by a local `cargo build -p astra-cli` and the staging script's placeholder mode; commit `feat: add the agent-first astra-cli with audited verbs`.

### Task 4: the sidecar package

**Files:**
- Create: `sidecar/{package.json, tsconfig.json, esbuild.config.mjs, src/index.ts, src/protocol.ts, src/protocol.test.ts, src/harness/{pi-adapter.ts, mock.ts}, src/exec/safeExec.ts, src/exec/safeExec.test.ts, src/permissions/{bridge.ts, bridge.test.ts}, src/tools/mailTools.ts, src/tools/mailTools.test.ts, src/skills/draft-emails.md}`
- Port from: kylins `kylins.client.agent-sidecar/**` trimmed per the spec D4 (drop memory modules, triage, rules; A-slices bring them).

**Interfaces:**
- Produces: the JSONL protocol module (TS mirror; types pinned against the Rust mirror's serde by a shared fixtures test; both sides parse the same fixture files, a new `fixtures/protocol/*.json` set committed under sidecar/src/protocol.test fixtures and consumed by the Rust test via include_str); safeExec (env scrub to a minimal allowlist + ASTRA_* passthrough, argv prefix check `astra-cli`, cwd at data dir); the permission bridge (policy table from init; every gated tool needs approval unless the table grants; fail closed on unknown); mailTools (the five tools calling safeExec with `--json --correlation-id` wired from the turn's id); MockHarness (fixture transcripts driving the protocol without the pi SDK); the pi adapter mounting the tools and one model registration (init's modelConfigs); the bundle builds to dist/index.cjs via esbuild (single file, CJS, no externals).

- [ ] Step 1: protocol.test first (each message shape round trips; unknown types error).
- [ ] Step 2: safeExec.test + bridge.test + mailTools.test first (stub CLI binary: a fixtures script echoing its argv/env subset as JSON; scrubbed env asserted; fail-closed gating; tool payloads carry the correlation id).
- [ ] Step 3: Implement; `node --test` green; the esbuild bundle builds and its byte-for-byte rebuild is stable; commit `feat: add the agent sidecar with safe exec and the permission bridge`.

### Task 5: host relay, supervisor, permission routing, panel

**Files:**
- Create: `client/desktop/src/agent/{mod.rs, relay.rs, relay_tests.rs, supervisor.rs, supervisor_tests.rs, permissions.rs, commands.rs, commands_tests.rs, protocol.rs}` (the Rust mirror of the sidecar protocol); Modify: `client/desktop/src/lib.rs` (commands + events wiring), `client/desktop/tauri.conf.json` (bundle the sidecar resource); Create: `client/web/src/features/agent/{AgentPanel.tsx, AgentPanel.test.tsx, agentStore.ts}` + services/ipc additions
- Port from: kylins `kylins.client.backend/src/agent/**` trimmed (relay/supervisor/permission_bridge/commands shapes) + the AgentPanel skeleton.

**Interfaces:**
- Produces: `SidecarRelay` (spawn `node dist/index.cjs` with env: model config from `ASTRA_LLM_*`, data dir, no secrets beyond the model key; init handshake -> ready; JSONL pump); supervisor (stale-generation rejection, linear backoff 1s/2s/4s capped, Lost state after budget, `sidecar-lost` event); `agent_prompt(prompt, correlation) -> prompt id` command (generates the correlation id when absent; streams `agent:event` Tauri events: text deltas, tool calls, permission requests with the correlation id); `agent_abort`, `agent_status`; the permission bridge routing to `agent:permission-requested` events and back via `permission_response`; `llm_call` envelopes written to the AiStore ledger; the AgentPanel (chat log with streaming, input, approval card dispatching the response; abort button).

- [ ] Step 1: supervisor/relay tests first with a stub sidecar script (a fixtures echo script exercising ready/event/done; generation rejection; backoff step count; Lost after budget).
- [ ] Step 2: commands_tests first (prompt round trip against the stub; correlation id generated and echoed in events; permission round trip; abort).
- [ ] Step 3: AgentPanel tests first (log renders streamed deltas; the approval card dispatches the response; abort dispatches).
- [ ] Step 4: Implement; all green; commit `feat: host the sidecar with supervision, permissions, and the agent panel`.

### Task 6: offline e2e, docs, capabilities

**Files:**
- Create: `client/desktop/tests/agent_e2e.rs` (or the crate's tests dir; the offline e2e: host + real sidecar bundle + the stub CLI from Task 4's fixtures; a scripted conversation: prompt -> text deltas -> gated save_draft approval -> tool_result -> done; assert the audit row and the llm_call row share the turn's correlation id); Modify: `docs/ai-foundation.md` (+ zh mirror), `docs/capabilities.md` (+ zh mirror), `docs/changelog/unreleased/ai-foundation.md`

- [ ] Step 1: e2e first (red: nothing hosts yet is Task 5's state; after T5 it goes green), run it; record output.
- [ ] Step 2: The manual LLM smoke checklist section in ai-foundation.md (the author's steps with their connection info; the checklist names every command).
- [ ] Step 3: capabilities rows (AI foundation rows partial; CLI rows partial) + zh mirror + fragment (Platforms: desktop, Bump: minor); ai-foundation.md carries the three principles, the registry, the traceability contract, the product-DB arrival, the eval-harness A1 deferral, and the known gaps (single model registration, no streaming client, DLP allow-only, the CLI second-writer seam).
- [ ] Step 4: Full battery (gate direct exit, workspace, doc, desktop, web, vitest, sidecar node tests, CLI build); commit `feat: prove the agent loop offline and record the ai contract`.

### Task 7: close-out

- [ ] Step 1: Full battery green; branch `spa0-ai-foundation` pushed; PR opened (squash-merge expected per the rulebook) linking the spec; acceptance evidence; ledger cleanup.

## Self-review notes

- Spec coverage: D1 (T1), D2 (T2), D3 (T3), D4 (T4), D5 (T5 + T3's staging), D6 (T1 correlation + T3 audit + T5 propagation + T6 e2e assertion), docs (T6).
- Interface chain: registry/store (T1) -> client + CLI (T2/T3, both consume T1) -> sidecar over CLI (T4) -> host (T5) -> e2e/docs (T6).
- Risks carried: pi SDK API surface at kylins' pin (T4 reads kylins' adapter verbatim before trimming); the Rust/TS protocol mirror drift (T4's shared-fixture test is the net); the CLI second-writer seam on the engine store (honest record in T3 + ai-foundation.md known gaps); Node discovery and version gate on CI (T5's supervisor tests cover the logic; CI runners have Node; the desktop job's node step exists already).
