# AGENTS.md

The single rulebook for Astra, written for humans and coding agents alike.
Every rule lives here or in the one document it names; everywhere else links.

## Product

Astra Mail is a desktop mail client. The product is named "Astra Mail",
never "Astra" alone in user-facing copy. Name and app id come from
branding/default.env at build time.

## Writing

- Brief and concrete. Say it once: a rule lives in exactly one place.
- British English (`colour`, `licence` noun, `license` verb). `dialog`,
  `artifact`, and `catalog` stay American.
- No dash as punctuation: no em or en dash anywhere, and no spaced hyphen
  as punctuation in Markdown.
- Code comments describe the present only; no history, no plan phases,
  sparingly. Log lines never mention repository paths.
- English only across documentation, code, comments, and folder names.
  `docs/zh/**` holds non-normative Chinese review copies; when a document
  changes, its review copy changes in the same commit.

## Hard rules

- Source files stay under 500 lines. The gate enforces it for the source
  languages it scans; Markdown prose is exempt.
- Newtypes for identities; no raw `String` ids across module boundaries.
- Test-first for behaviour changes. A bug fix lands with a regression test
  at the lowest shared layer that observes the contract.
- The engine is consumed only through `engine-api`. Protocol parsing belongs
  in the engine, never in Astra.
- Intents stay serde-serialisable. Surfaces are immutable snapshots.
- `docs/capabilities.md` moves in the same change as the feature it
  describes.
- `VERSION` moves only in a release PR and always together with the
  workspace package version.

## Contract changes

A contract is decided once in its document under `docs/` and binds the whole
product. Changing a contract takes: (1) the doc rule and matrix update,
(2) the implementation on every surface that ships it, (3) the
`docs/capabilities.md` row, (4) a changelog fragment in
`docs/changelog/unreleased/<slug>.md` with `Platforms:` and `Bump:` tags.
Shortfalls are recorded as known gaps, never left silent.

## Changelog fragments

One file per user-visible change: `docs/changelog/unreleased/<slug>.md`
containing the note, then `Platforms: desktop` and `Bump: none|minor|major`.
Fragments move to `docs/changelog/released/` in the release PR. Fragments
are English only and have no Chinese mirror.

## Security

kylins implementations win on security mechanics where they have the
advantage; mail-calendar wins on feature contracts. Judges gate user
experience only, never authorisation; the permission bridge, send
refusal, and CLI gating stay deterministic. The sidecar is fail
closed: every gated tool starts as needs-approval, send-class verbs are
refused to the agent, and humans decide and send. Every LLM call, tool
call, and permission decision lands in its ledger with a correlation id.

## AI rules

- An AI behaviour change (prompt, pipeline, tool surface) requires an eval
  run in the same PR and a pipeline version bump.
- Model API keys live only in sidecar memory. Background tiers run from the
  host with the shared tier and use-case registry.

## CLI rules

`astra-cli` is the agent's tool-call entry point, designed agent-first.
Every verb ships with `--json` (versioned schema) and mutating verbs ship
with `--preview`. New verbs land in the same change as the feature they
expose, together with any new virtual permission tool.

## Plugin rules

Plugins run out of process against the versioned hook API in
`docs/plugins.md`. Hooks never touch the engine; send discipline cannot be
bypassed. Hook API changes follow the versioned process in that document.

## Upstream sync

References are locked per `refs/refs-lock.toml`; the process and the 22:00
task contract live in `docs/upstream-sync.md`. Silent deviation from a
reference decision is forbidden: disagreements are escalated in the PR.

## Build and verify

- Git Bash on Windows is the shell; bash 5 or newer.
- `cargo run -p xtask -- gate` before every push. CI mirrors the gate.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` are the floor.
- Traps discovered while working go into this section, appended one line
  each.
- rust-toolchain.toml overrides the CI toolchain: the pinned channel must
  list the components it needs, or fmt and clippy go missing on the runner.
- The gate walks the working tree, not the git index; .superpowers is
  skipped, so nothing tracked may live there.
- rustc dead_code analysis ignores test-only usage: a helper whose runtime consumer lands in a later task stays pub in a private module, never pub(crate), or deny(warnings) fails the interim build.
- tauri-build writes its capability schemas into client/desktop/gen in the working tree, not OUT_DIR; the gate skips that directory and it stays untracked.
- npm 11 blocks esbuild's postinstall until approved: run `npm install-scripts approve esbuild` in client/web after a fresh install, or the dev server stays unbuilt.
- The allowScripts record in client/web/package.json is what that approval writes; npm 11.19 only warns when it is missing, and the committed record keeps fresh installs and CI silent.
- rustdoc denies public docs linking private items (private_intra-doc_links): name a private constant in prose, never as a doc link, or `cargo doc --workspace --no-deps` fails.
- vitest fake timers freeze React's scheduler: a zustand set between assertions never re-renders; wrap mid-test store mutations in act() from @testing-library/react.
- astra-desktop bundles astra-cli as externalBin and tauri-build checks the staged file exists on every cargo invocation of the crate: run node client/desktop/scripts/stage-cli.mjs --placeholder-ok once on a fresh checkout, or even cargo check -p astra-desktop fails.
- Windows recycles process ids and never cleans the agent testkit's temp parent, so a recycled pid revisits a stale scratch dir; the first scratch() call for a name in a process wipes it (later calls reuse the live directory) or the e2e's nothing-executed-yet assertion fails on a morning-old capture file.
- Node on Windows injects SYSTEMROOT, HOMEDRIVE, TEMP, and friends into every spawned child even when the spawn env omits them; env-scrub assertions must allow that fixed platform set.
- Git Bash printf collapses backslash escapes inside single-quoted arguments on this host; write JSON fixtures containing Windows paths with node, never printf.
- sidecar npm test runs its e2e file against dist/index.cjs; run npm run build in sidecar first or those tests fail on a fresh checkout.
- snippet and knowledge rows stamped in the same second tie-break on id DESC (minted UUIDs), so a test cannot assert recency order across same-second inserts: seed fixed ids or compare sets.
- the engine outbox dedups edit ops by (account, idempotency key) across every op state, so a test helper reusing its idempotency keys within one engine gets the settled op back and the send claim refuses it as Settled: mint a fresh key per edit.
- astra-desktop bundles the sidecar as a Tauri resource and tauri-build checks sidecar/dist exists on every cargo invocation of the crate, clippy included: build the bundle (npm --prefix sidecar run build) on a fresh checkout before compiling, or the build script fails.
- Node on Windows asserts in ncrypto::CSPRNG when a spawn omits SYSTEMROOT, and the variable usually arrives uppercase: the agent spawn's platform allowlist matches names case-insensitively or every sidecar dies at once.
- AppState's field order is its drop order and it is load bearing: the agent relay must drop before the runtime, or the runtime's shutdown waits on a read pump that waits on the child that waits on the relay's stdin closing.
- Pull requests merge with squash and merge, subject "PR title (#number)",
  branch deleted after; merge commits are not used.
- xtask doc_sync self-test fixtures use pid-keyed temp dirs that are never cleaned; Windows pid reuse can pollute the stale dirs and flake concurrent runs; serial reruns are clean; a fixture-helper cleanup is the eventual fix.
- A trait impl cannot split across modules (E0119): when a fake grows past the file cap, its whole `impl AppPort for FakeApp` block moves to the child module; the root keeps the struct, inherent helpers, and re-exports.
- client/web/src/services/ipc.ts sits at the line cap: new wire types and wrappers land in the owning feature service (ai.ts, sending.ts), never in ipc.ts itself.
