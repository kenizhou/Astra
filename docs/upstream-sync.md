# Upstream sync contract

How Astra tracks its four reference projects. The rulebook (AGENTS.md) links
here; this document is the single place the sync process is described.

## Lock table

`refs/refs-lock.toml` records, per reference: `url`, `path` (relative to
`root`), `baseline` (the commit locked at bootstrap), `last_ported` (advances
as ports land), and `locked_at`.

## Lock phase

References are frozen at their baselines until feature parity: every
mail-calendar row in docs/capabilities.md is complete or explicitly N/A, AI
phases A1 to A4 are done, and the plugin host is in place. Until then no
reference is pulled forward into Astra.

## The 22:00 task

A local scheduled automation runs nightly at 22:00, idempotent, anchored on
`last_ported`:

1. Fetch all four references; diff `last_ported..HEAD`; skip when empty.
2. Classify changes: docs only, tests only, refactor, behaviour, new feature,
   security.
3. Port per the rulebook. Contract doc, capability matrix row, changelog
   fragment, and the Chinese review copy of any document land in the same
   change.
4. Security tagged changes are always held for review and never auto-merged.
5. Run the fast local gate subset; the full gate runs in CI.
6. Open a PR with a per-repository summary. Auto-merge only low-risk classes
   with a green gate; everything else waits for morning review with a digest.
7. On merge, advance `last_ported`.
8. UI style decisions and architectural conflicts are listed in the PR for
   the author to decide. Silent deviation is forbidden.

## Engine updates

The engine is git-pinned by revision in the workspace Cargo.toml. An upstream
update arrives by bumping the pin, adapting the core, and following the same
PR flow as any other port.
