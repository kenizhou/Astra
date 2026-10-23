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

The engine is vendored in-tree at `vendor/engine/` (the full upstream tree
at the revision the lock table records, MPL-2.0), so Astra's engine
modifications land as ordinary commits on main and upstream changes arrive
as reviewable merges instead of a manual re-pin. The loop:

- **`vendor-sync` is the pristine branch.** It always holds the upstream
  tree at the last synced revision; `main` carries Astra's in-tree edits on
  top. A persistent worktree OUTSIDE this repository
  (`D:/Projects/mailclient/Astra-vendor-sync`, so the gate never walks it)
  is where the nightly script works.
- **Nightly (the 22:00 job).** `scripts/dev/upstream-sync.sh` fetches
  upstream, reads the current vendored revision from `vendor-sync`'s last
  `vendor: upstream <rev>` commit message (never from `last_ported` alone,
  so a sync PR left open for nights neither resends a stale digest nor
  skips a revision), and no-ops when equal. Otherwise it `git archive`s the
  new revision into the worktree, commits `vendor: upstream <rev> <date>`,
  pushes, and opens (or updates) the `vendor-sync -> main` PR with a
  classified digest of the range: docs only, tests only, ci-docker,
  dependencies (`Cargo.toml`, `Cargo.lock`), behaviour, security. The
  dependencies bucket names rusqlite/libsqlite3-sys explicitly: the two
  stores share one libsqlite3-sys, so an upstream rusqlite bump can break
  Astra in the same build.
- **Sync PRs merge with a merge commit, never squash.** A squash merge
  means the branch tip never becomes an ancestor of main, every later sync
  re-diffs the whole tree, and the conflict-surfacing mechanism dies. (The
  fallback, `git merge -s ours main` on `vendor-sync` after each squash, is
  recorded but not the rule.)
- **Conflicts surface in the PR, never silently.** When an upstream change
  touches lines Astra has edited in-tree, the merge in the sync PR shows
  it. The runbook: `git checkout vendor-sync && git merge main`, resolve
  `vendor/engine` conflicts keeping Astra's edits, push; the PR updates.
  `git diff vendor-sync main -- vendor/engine` always lists every in-tree
  modification, so silent deviation from upstream is impossible by
  construction.
- **On merge, advance `last_ported`.** The lock table's engine row stays
  the provenance record.
- **The first in-tree delta is the MIME-wrap seam** (the ECS spec, landed
  2026-10-23): `engine-provider`'s `MimeWrapper` trait and the
  serde-defaulted `Draft::sign`/`Draft::encrypt` flags, provider-imap's
  `wrap` module (the wrapper consulted pre-dial, the Sent-copy repair
  re-wrapping), and a `PendingOutcome::Failed` detail so a refused op's
  row says why. `git diff vendor-sync main -- vendor/engine` lists it;
  every sync PR review should expect these files to differ and treat any
  upstream touch to them as a conflict to resolve deliberately.
- **The engine's own battery is not part of `cargo test --workspace`.**
  The vendored tree is a nested workspace excluded from membership, so its
  tests run only explicitly: `cd vendor/engine && cargo test --workspace`
  (live-server tests skip without their environment). A sync PR's review
  checklist runs it, the Astra battery, the xtask gate, and the live
  harness.
- **Build directories.** This repository shares one build directory across
  checkouts, with freshness keyed on workspace-relative paths. Any
  worktree whose `vendor/` diverges from another checkout's (the
  vendor-sync worktree above all) must set `CARGO_BUILD_BUILD_DIR` to its
  own directory, or a build can be handed stale engine artifacts.
- Security-classified syncs are always held for review and never
  auto-merged, matching the 22:00 contract's rule for every reference.

### The 22:00 wiring

A Windows Task Scheduler job runs the driver nightly, logging OUTSIDE the
repository (`D:/Projects/mailclient/logs/vendor-sync.log`):

```bash
schtasks /Create /F /TN "Astra\vendor-upstream-sync" /SC DAILY /ST 22:00 \
  /TR "\"C:\Program Files\Git\bin\bash.exe\" -lc 'D:/Projects/mailclient/Astra/scripts/dev/upstream-sync.sh >> D:/Projects/mailclient/logs/vendor-sync.log 2>&1'"
```

`upstream-sync.sh --dry-run` prints the current and new revisions with the
classified digest and changes nothing; it is the way to watch a sync before
letting it land. Any failure (a locked file, the network, `gh` auth, a
failed pristine proof) exits non-zero into the log and is a morning-review
item, never a silent skip.
