# SPA0 + SP-CLI plan: closing the two placeholder rows

Spec: docs/superpowers/specs/2026-11-11-spa0-sp-cli-closure-design.md. Branch
`spa0-sp-cli-closure` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit. Both features are
contract changes: cli.md and capabilities.md move with their zh mirrors in
the same commit, each with its own changelog fragment.

### Task 1: the permission gating map (SPA0, D1)

**Files:** docs/cli.md (the new "The permission gating map" section: the
exhaustive ten-row table tool / verb+subverb / level / preview shape, the
fail-closed rule, the exhaustive-match rule; reconcile the matrix row at
251 to point at the section) + docs/zh/cli.zh-CN.md;
client/desktop/src/agent/permissions.rs (the five new `shaped_arg_preview`
arms: search_messages, mark_read, set_flagged, archive_message,
move_to_trash; the generic fallback stays; the ten-shape pin in the test
module); client/desktop/src/agent/policy_tests.rs (the `default_policy()`
pin: exactly the ten tools at the documented levels);
sidecar/src/tools/mailTools.test.ts (the ten-name pin and the per-tool
`buildCliArgv` verb/subverb pin); docs/capabilities.md:152 ⬜ to ✅ + zh;
docs/changelog/unreleased/spa0-permission-gating-map.md (Platforms: desktop,
Bump: none).

- [ ] Tests first (the policy pin, the ten shapes, the sidecar pins red);
      then the five arms and the doc map; the offline agent e2e and policy
      tests stay green. Commit `feat: pin the virtual permission tools mapping`.
- [ ] Cap watch: permissions.rs sits at 384 lines; if the arms plus tests
      cross 500, move the test module to a sibling (a free fn, so no E0119
      constraint). mailTools.ts stays untouched (the pin lives in its test
      file). No model-facing change: no eval, no pipeline bump.

### Task 2: shell completions (SP-CLI, D2)

**Files:** cli/src/verbs/verb.rs (the `Completions { shell }` variant, its
`name()` arm "completions"; `mutating()` stays false);
cli/src/verbs/parse.rs (the `completions <shell>` match arm; an unknown
shell is a usage error; `--preview` refused by the shared gate);
cli/src/verbs/completions.rs (the generator: the four shell scripts from
`Verb::name()`, `SUBVERBS`, and the per-verb flag descriptor; the exhaustive
match over `Verb`); cli/src/verbs/completions_tests.rs (the run()-layer
reds); cli/src/verbs/mod.rs (declare the modules; the execute arm answering
before the engine opens, the store verbs' discipline; the run result gains
the raw-script field); cli/src/main.rs (print the raw script when present
and `--json` absent, else the envelope); docs/cli.md (the verb table row,
the count thirty-one to thirty-two, the matrix completions row, the
envelope section's generator exception, the recorded clap deviation) + zh;
docs/capabilities.md:153 ⬜ to ✅ + zh;
docs/changelog/unreleased/sp-cli-shell-completions.md (Platforms: desktop,
Bump: minor).

- [ ] Tests first (the four shells name every verb; unknown shell exit 2;
      `--json` schema-1 envelope with data.shell/data.script; `--preview`
      usage error; the raw form is the script not an envelope; the
      descriptor covers every verb and matches parse for the representative
      verbs; the audit row lands); then the generator and the wiring.
      Commit `feat: ship shell completions from the verb vocabulary`.
- [ ] Cap watch: parse.rs at 438 and verbs/mod.rs at 429 stay under 500 with
      the small arms; if the four shell templates plus the descriptor cross
      500 in completions.rs, split the templates into a child module. The
      flag descriptor is spelled twice (parse + descriptor) and pinned by a
      drift test, the CLOSED_ACTIONS precedent.

### Task 3: close-out

- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate. No live model proof (no model-facing behaviour
      moves).
- [ ] Branch pushed; PR (squash expected); CI watch (ci-ok, desktop,
      sidecar); squash-merge, delete branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's map + host pins + sidecar pin + row (T1); D2's verb +
  generator + descriptor + output exception + row (T2); battery + PR (T3).
- Risks: the five preview arms must not disturb the offline e2e (it pins
  save_draft, whose shape does not move) nor the policy tests; the raw
  output field must not change any other verb's envelope (the byte pins in
  envelope_tests.rs stay green); the completions verb must answer without
  opening the engine (a cold data dir works); the flag drift pin must fail
  when parse and the descriptor disagree.
- Deviations recorded in the spec, cli.md, and the PR: the clap_complete
  sentence (generate from the vocabulary instead) and the one generator
  exception to the envelope rule. The no-eval judgement (host-side preview
  only, model-facing surface unchanged) is recorded in the spec.