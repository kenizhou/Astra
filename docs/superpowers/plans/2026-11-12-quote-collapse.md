# Quote collapse plan: folding the quoted original

Spec: docs/superpowers/specs/2026-11-12-quote-collapse-design.md. Branch
`quote-collapse` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit. A contract change:
reading-zoom.md gains a quote-collapse section with its zh mirror in the
same commit, capabilities.md moves with its zh mirror, the porting-backlog
entry resolves, and a changelog fragment lands.

### Task 1: the transform and wiring (D1, D2)

**Files:** client/web/src/components/reading-pane/collapseQuoted.ts (the
pure transform: trailing blockquote chain + the preceding "On ... wrote:"
attribution + a trailing .gmail_quote, folded into a collapsed
details/summary with a fixed constant label; the two safety no-ops; the
fragment wrapped, never reparsed or re-encoded); collapseQuoted.test.ts (the
pure-transform reds, exact output pins); MessageHtml.tsx (call collapseQuoted
at the composition point before wrapBodyDocument, on the gated html; NOT
inside wrapBodyDocument, so print keeps the full body); MessageHtml.test.tsx
(the component reds: the srcdoc carries the details, the opt-in rewrite
restores srcs inside it, the hostile fragment rides verbatim, print does not
fold).

- [ ] Tests first; then the transform and the wiring. Commit
      `feat: collapse the quoted original in the reading pane`.
- [ ] Cap watch: collapseQuoted.ts is small (~40 lines); MessageHtml.tsx is
      260 and stays well under 500. No sanitiser change, no CSP/sandbox
      change, no new intent, ipc.ts untouched. The existing wrapBodyDocument
      and enableRemoteImages byte pins stay green.

### Task 2: the contract + close-out (D3)

**Files:** docs/reading-zoom.md (a quote-collapse section: the detection,
the default-collapsed fold, the print exemption, the verbatim guarantee) +
docs/zh/reading-zoom.zh-CN.md; docs/capabilities.md (a new Mail-area row
beside the reading rows, ✅) + docs/zh/capabilities.zh-CN.md;
docs/porting-backlog.md (the quote-collapse entry resolves) + its zh mirror;
docs/changelog/unreleased/quote-collapse.md (Platforms: desktop, Bump:
minor).

- [ ] The docs + zh + row + backlog + fragment. Commit
      `docs: the quote collapse contract`.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate. No live model proof (pure UI).
- [ ] Branch pushed; PR (squash expected); CI watch (ci-ok, desktop,
      sidecar); squash-merge, delete branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's transform + D2's wiring (T1); D3's contract + row +
  backlog + fragment + battery + PR (T2).
- Risks: the fold must wrap the fragment verbatim (no re-encoding; the
  hostile-verbatim test pins it); the injected markup is a fixed constant
  (no sender-derived attribute); the remote-image placeholders must survive
  and the opt-in rewrite must still find them inside the details; print must
  keep the full body (the fold stays out of wrapBodyDocument); the CSP and
  sandbox do not move; the whole-body-quote and no-quote no-ops hold.
- No eval, no pipeline bump (pure UI). No sanitiser change (the fold is
  app-owned markup injected after sanitisation).