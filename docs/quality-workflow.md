# Quality workflow

The operational runbook that wires the gstack skill suite into Astra's slice
cadence. AGENTS.md stays the normative rulebook; this document says when to
run which skill, in which order, against which surface. It moves in the same
change as any workflow step it describes.

## Prerequisite: the fixture-backed design harness

The webview runs only inside the Tauri host, and the visual skills
(`/browse`, `/design-review`, `/design-shotgun`, `/qa`, `/benchmark`) drive a
real Chromium. The fixture harness closes that gap and ships:

- `npm run dev:fixtures` (vite's `--mode fixtures`) aliases the Tauri bridge
  to `client/web/src/services/__fixtures__/harness.ts`, a stub that feeds the
  JSON snapshots in its directory into the stores and settles mutations
  quietly. The server on localhost serves the real UI with realistic data,
  no Tauri host and no live accounts. Its contract is pinned by
  `harness.test.ts` beside it.

## Once, at programme start

1. `/design-consultation` to fix the design system (typography, colour,
   spacing, motion) as tokens under `client/web/src/styles/`. Every later
   surface draws from these tokens instead of inventing its own.

## Per slice (SP2m onward)

Design first, then implementation, then verification, all inside the one
change the rulebook's contract process demands.

1. `/spec` to sharpen the slice scope beside the superpowers spec.
2. Mock the new surfaces against the harness: `/design-html` for a single
   surface, `/design-shotgun` when the direction is contested and variants
   need a comparison board.
3. `/plan-design-review` on the slice plan. For slices with wide blast
   radius (OAuth, plugins, the MCP surface), run `/autoplan` instead to get
   the CEO, engineering, design, and developer experience reviews in one
   pass.
4. Implement, test-first, exactly as the rulebook demands.
5. `/design-review` on the built surface through the harness; fix what it
   finds before the slice PR opens.
6. If the slice changes AI behaviour (prompt, pipeline, tool surface), the
   eval run lands in the same PR with the pipeline version bump, unchanged
   from the rulebook. Fixture evals stay free; `/eval-plan` preregisters any
   paid live-model run (bars, arms, decoys, stop rules, priced pilot) before
   money moves.
7. Contract doc, `docs/capabilities.md` row, and changelog fragment in the
   same change, as the rulebook demands.

## Per PR

1. The floor, unchanged: `cargo fmt --all`,
   `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, `cargo run -p xtask -- gate`.
2. `/review` on the diff. The gate enforces contracts; the review judges
   the change.
3. `/security-review` on any diff touching a trust boundary (the table
   below). A boundary diff without a security review does not merge.
4. Squash merge, subject "PR title (#number)", branch deleted.

Optional guardrails: `/freeze` scoped to the slice's directories keeps a
long slice's edits inside its boundary.

## Per milestone (end of each slice, and the lock phase)

1. `/cso` against each trust boundary below, one pass per boundary. Findings
   land as fixes in the same milestone or are recorded as known gaps in the
   boundary's contract doc, never left silent.
2. `/test-audit` to cull low-value and duplicate tests and the test-only
   code they keep alive.
3. `/deslop-shared-libs` to find shared extractions worth making across the
   CLI verbs, desktop commands, MCP tools, and plugin hooks that all wrap
   the same astra-app surface.
4. `/health` for the quality trend over the gate, clippy, and vitest floor.
5. `/benchmark` on the webview bundle through the harness, so a slice that
   bloats the UI is caught at the milestone, not in user hands.

### The trust boundaries

| Boundary | Where it lives |
|---|---|
| Untrusted message content fence | crates/astra-mcp (`fence`, `tag_untrusted` in astra-ai) |
| Reading pane HTML sanitiser | the ammonia configuration behind the reading view |
| DLP redaction gate | crates/astra-ai (`DlpPolicy`, `redact`, `apply`) |
| Sidecar permission bridge | sidecar/src/permissions (fail closed, needs-approval default) |
| Send refusal | no send-class verb reaches the agent; pinned by tests in astra-mcp and the CLI |
| Plugin permission table | crates/astra-plugin (manifest, hook API, approval defaults) |
| OAuth PKCE and token handling | crates/astra-oauth and the host sign-in session |
| Credential storage | the OS keyring seam in the desktop host |
| CLI gating | cli verbs, `--preview` on mutation, the audit envelope |

The deterministic invariants (send refusal, needs-approval defaults) stay
pinned by Rust tests; `/cso` hunts what the tests do not model.

## Weekly and monthly

- Weekly: `/retro` over the week's slices and traps.
- Monthly: `/learn` to review and prune accumulated learnings; `/test-audit`
  if a milestone skipped it.

## Recorded conflicts with the tooling defaults

- `/ship` bumps VERSION and edits the changelog; the rulebook moves VERSION
  only in a release PR and takes changelog entries as fragments. Use `/ship`
  with its version step suppressed, or land manually.
- `/document-generate` writes docs after the fact; the rulebook moves
  contract docs in the same change as the feature. Use it only for
  non-contract prose, inside the slice PR.
- `/benchmark-models` compares models for the tier table in astra-ai; it
  informs `TierTable` configuration and never ships user data.
