# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## The rulebook

`AGENTS.md` is the single rulebook for this repo: writing style (British English, no em/en dashes), hard rules (500-line file cap, newtypes for identities, test-first, changelog fragments, contract-change process), the security posture for the AI surfaces, and a long list of environment traps discovered on this Windows host. Read it before changing behaviour; traps discovered while working are appended to its "Build and verify" section, one line each.

## Build and verify

Git Bash on Windows is the shell.

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p xtask -- gate          # run before every push; CI mirrors it
cargo run -p xtask -- check <name>  # one gate check only (names in xtask/src/checks/)
```

Single test: `cargo test -p <crate> <test_name>`. Rust tests sit beside the code in `*_tests.rs` sibling modules.

Fresh-checkout prerequisites (without these, even `cargo check -p astra-desktop` fails, because tauri-build verifies staged artifacts on every cargo invocation of the crate):

```bash
node client/desktop/scripts/stage-cli.mjs --placeholder-ok
npm --prefix sidecar ci && npm --prefix sidecar run build
```

Web client (React/Vite, in `client/web`):

```bash
npm --prefix client/web ci          # esbuild postinstall approval is committed in package.json allowScripts
npm --prefix client/web run test:run
npm --prefix client/web run build
npm --prefix client/web run dev:fixtures   # the real UI in a plain browser against fixture snapshots (design reviews, QA)
```

Sidecar: `npm --prefix sidecar test` runs its e2e against `dist/index.cjs`; build first.

Live harness (seeded Stalwart mail server in docker/harness; gated live tests skip unless `ASTRA_LIVE_HARNESS=1`):

```bash
scripts/dev/harness.sh up
eval "$(scripts/dev/harness.sh env)"
cargo test -p astra-app tests_live_harness -- --nocapture --test-threads=1   # serial is the recorded discipline
scripts/dev/slice-check.sh   # SP1 vertical slice: headless desktop against the harness
```

Desktop host: `cargo build -p astra-desktop` (heavy Tauri tree; excluded from default workspace members). CLI: `cargo run -p astra-cli -- <verb> [--json] [--preview]`.

## Architecture

Astra Mail is a desktop mail client being ported, slice by slice, from the mail-calendar reference (see `docs/upstream-sync.md`; work is organised as SP-prefixed slices with specs and plans under `docs/superpowers/`).

**Layering, bottom to top:**

- `engine-api` (external crate, pinned by git rev in the workspace `Cargo.toml`) is the only door to the sync engine. Protocol parsing (IMAP, SMTP, CalDAV, JMAP, Graph, Google) lives in the engine, never in Astra. Re-pinning means bumping the identical `rev` on every engine line at once.
- `crates/astra-account` turns a stored TOML account config (secrets via the OS keychain) into dialled engine connections; also autodetect and OAuth (`astra-oauth`, `astra-autodetect`).
- `crates/astra-app` is the application core: a unidirectional loop. A host observes a `Surface` through `AppObserver`, pulls an immutable snapshot, and dispatches an `Intent` back. Intents stay serde-serialisable.
- `crates/astra-viewmodel` holds pure, immutable snapshots shared by every surface.
- Hosts: `client/desktop` (Tauri 2 Rust host, crate `astra-desktop`) and `cli` (`astra-cli`, the agent-first tool-call entry point; every verb has `--json`, mutating verbs have `--preview`). The webview UI is `client/web` (React 19, zustand, Tailwind 4, tiptap), talking to the host over Tauri IPC via `src/services/ipc.ts` (at the 500-line cap; new wire types land in the owning feature service such as `ai.ts` or `sending.ts`, never in `ipc.ts`).

**AI surfaces (fail closed by design):**

- `sidecar/` is a Node.js process (esbuild bundle, pi-coding-agent harness) the desktop host spawns and drives over JSONL stdio. Every gated tool starts as needs-approval; send-class verbs are refused to the agent; humans decide and send.
- `crates/astra-ai` is the shared foundation: use-case registry, model tier table, `CorrelationId`, the product SQLite store (ledgers, rules, draft pool, knowledge, snippets), DLP redaction, and the judge vocabulary. It never calls a model itself.
- `crates/astra-mcp` (+ `astra-mcp-relay`) is the MCP server surface; there is deliberately no `send_message` tool. Message bodies are wrapped in an `<untrusted-message-content>` fence before reaching a model.
- An AI behaviour change (prompt, pipeline, tool surface) requires an eval run in the same PR (`evals/`) and a pipeline version bump.
- `crates/astra-plugin` is the plugin contract skeleton: out-of-process plugins, versioned hook API, hooks never touch the engine.

**AppState drop order is load bearing** (`client/desktop/src/app_state.rs`): the agent relay must drop before the runtime, or shutdown deadlocks.

## Contracts and docs

A contract is decided once in its document under `docs/` and binds the whole product. Changing one takes the doc update, the implementation on every shipping surface, the `docs/capabilities.md` row, and a changelog fragment in `docs/changelog/unreleased/<slug>.md` (`Platforms:` and `Bump:` tags), all in the same change. `docs/zh/**` holds non-normative Chinese review copies that change in the same commit as their English source. Feature work follows specs/plans in `docs/superpowers/specs/` and `docs/superpowers/plans/`.

PRs merge with squash, subject "PR title (#number)", branch deleted after; no merge commits.
