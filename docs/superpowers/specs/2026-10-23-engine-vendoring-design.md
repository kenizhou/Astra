# Engine vendoring design: the sync engine moves in-tree (vendor/engine/)

**Status.** Decided 2026-10-23. Branch `engine-vendoring` from the SP4b
tip (SP4b carries the `engine-rfc5322` dependency line this switch
rewrites). User decisions 2026-10-23: vendor the engine's FULL tree (all
22 crates plus fuzz/, docker/, tools/, docs/, scripts/ and the licence
files, everything but `.git` and build artifacts); run the nightly
incremental sync as a local scheduled job (the documented 22:00 contract
in docs/upstream-sync.md, not a GitHub Actions cron); and carry Astra's
engine modifications **in-tree**, with a `vendor-sync` merge branch
holding the pristine upstream copy so nightly sync conflicts surface in a
PR instead of being silently dropped.

**Why the switch.** Today the engine arrives as ten git-pinned
dependencies in the workspace root `Cargo.toml`, all at
`68eab00e6a9a9fe5a200218e156dbaae1ba00caa` (eight more crates arrive
transitively). The pinned model blocks two things the roadmap needs:
Astra-side engine modifications without forking (the SP4b crypto outbox
already records the missing raw-MIME submit seam as a known gap the
pinned engine cannot close), and a living incremental sync instead of a
manual re-pin. Vendoring the tree at the pinned revision makes the switch
behaviour-neutral, and the `vendor-sync` branch plus the 22:00 job make
every later upstream change a reviewable merge PR. `refs/refs-lock.toml`'s
engine row stays the provenance record (baseline = `last_ported` =
68eab00).

**Scope.** Four PRs: the vendor switch at 68eab00 (PR 1), the sync
tooling and its dry-run (PR 2), the first real sync to upstream HEAD via
the mechanism (PR 3), and the scheduled job (PR 4). What is NOT in scope:
changing how Astra consumes the engine (the `engine-api`-only door
stands), moving any Astra code into `vendor/`, or joining the engine's
own test battery into Astra's workspace battery (see D6).

## D1: vendor/engine/ mirrors the upstream tree wholesale

The vendored tree is a `git archive` of upstream at the vendored
revision, extracted at `vendor/engine/`: all 22 crates (`engine-*`,
`provider-*`, `store-sqlite`, `stalwart-harness`, `mailbox-fixture`,
`dav-cli`, `engine-cli`), plus `fuzz/` (its own nightly-only workspace,
inert), `docker/`, `tools/`, `scripts/`, `docs/`, the upstream root
manifest, licence files, CI files, and dotfile provenance. Nothing
Astra-specific ever lives under `vendor/`: every piece of glue (the path
dependencies, the exemptions, the sync script) lives outside it, so a
sync PR's diff is always vendor-only.

The copy is made with `git -C <upstream> archive <rev> | tar -x -C
vendor/engine` (exactly the tracked files at the revision; no `.git`, no
untracked junk), never a filesystem copy. `.github/` and `Cargo.lock` come
along as inert provenance (they only act when a build runs inside
`vendor/engine`).

## D2: the nested workspace is load-bearing (exclude + upstream manifest)

Two Cargo facts, verified against the repo, shape everything:

- Cargo automatically enlists path dependencies inside the workspace
  directory as workspace members. The vendored tree is itself a
  workspace (its root `Cargo.toml` has `[workspace] members =
  ["crates/*"]`), and a member may belong to exactly one workspace. So
  Astra's root `[workspace]` must declare **`exclude = ["vendor"]`** or
  the workspace does not resolve at all.
- `version.workspace = true`, `edition.workspace = true`, and
  `[lints] workspace = true` resolve against the first workspace root
  found walking up from the package's own manifest. With the upstream
  root manifest vendored, the engine crates keep their own lint table,
  edition 2024, and MPL-2.0 licence metadata; Astra's stricter lint table
  (pedantic, `missing_docs = "deny"`) applies to Astra members only.
  Vendoring the upstream root manifest is therefore doubly required, not
  optional.

The upstream `rust-toolchain.toml` (1.97.0) governs only commands whose
cwd is inside `vendor/engine` (rustup resolves from cwd); Astra builds
keep Astra's pinned toolchain, unchanged from today's git-dep builds.

## D3: the dependency switch is ten git lines to ten path lines

The workspace root's `[workspace.dependencies]` block replaces the ten
git-pinned lines with path lines into the vendored tree, features
preserved (`engine-tls` keeps `tls-native-certs`). Member crates keep
their `engine-x.workspace = true` lines untouched. `Cargo.lock`
regenerates (the 18 `source = "git+...allodia-eu..."` entries vanish).
The commented-out `[patch]` block in `.cargo/config.toml` is deleted:
path dependencies need no patch (and the block was already missing
`engine-rfc5322`); its header comment instead records that in-tree engine
development means editing `vendor/engine`, with the vendor-sync flow
protecting provenance.

xtask's gate gains `vendor/` exemptions where upstream content would trip
it: `license_headers` (VENDORED_PREFIXES, own licences apply), `file_length`
(UPSTREAM_EXEMPT_PREFIXES, byte-diffable with upstream), `british_english`
and `dash_hygiene` (upstream prose and CI files). `doc_sync` needs no
change (its scope is AGENTS.md, `docs/*.md`, and `docs/superpowers/**`;
vendored docs owe no zh mirrors). `paths_ascii` keeps scanning `vendor/`
as a free standing guarantee.

## D4: the vendor-sync branch and the nightly merge PR

A `vendor-sync` branch always holds a pristine copy of upstream at the
last synced revision; `main` carries Astra's in-tree modifications on
top. A persistent worktree OUTSIDE the repository
(`D:/Projects/mailclient/Astra-vendor-sync`, so the gate never walks it)
is where the nightly script works.

`scripts/dev/upstream-sync.sh` (bash, idempotent, `--dry-run` supported)
runs the loop: fetch upstream; read the current vendored rev from
`vendor-sync`'s last `vendor: upstream <rev>` commit message (falling
back to refs-lock's baseline; the PR 1 import commit carries the same
format); no-op when equal; otherwise `git archive` the new revision into
the worktree's `vendor/engine`, commit `vendor: upstream <rev> <date>`,
push, and open (or update) the `vendor-sync -> main` PR with a classified
digest of the rev range: docs-only / tests-only / ci-docker /
dependencies (`Cargo.toml`, `Cargo.lock`: the rusqlite watch item) /
behaviour / security-keyword hits. Security-classified syncs are flagged
in the PR body and never auto-merge. The script never merges main into
`vendor-sync`, never force-pushes, and any failure exits non-zero into
the log for morning review.

**Sync PRs merge with a merge commit, never squash**: a squash merge
means the branch tip never becomes an ancestor of main, every later sync
would re-diff the whole tree, and the conflict-surfacing mechanism dies.
(The squash fallback, `git merge -s ours main` on `vendor-sync` after
each merge, is recorded but not chosen.)

Anchoring on `vendor-sync`'s own history (not `last_ported`) means a sync
PR left open for nights neither resends a stale digest nor skips a
revision; `last_ported` advances on main when the PR merges.

## D5: Astra's own edits stay in-tree, conflicts surface in the PR

Local modifications to `vendor/engine` land on main as ordinary commits
(the in-tree-edit flow the switch exists for). When a nightly sync's
merge touches the same lines, the `vendor-sync -> main` PR shows the
conflict; the runbook resolves it by hand (`git checkout vendor-sync &&
git merge main`, keep Astra's edits in `vendor/engine`, push) and the PR
updates. Silent deviation is impossible by construction: the pristine
branch is always one `git diff vendor-sync main -- vendor/engine` away
from a complete list of Astra's engine modifications.

## D6: the engine's own battery runs inside vendor/engine

With `exclude = ["vendor"]`, the engine crates are non-member path
dependencies, so their unit tests do NOT join Astra's
`cargo test --workspace` (the cms precedent runs only because cms is an
accidental member). The sync PR verification runs the engine battery
inside `vendor/engine` explicitly, where the nested workspace stands
alone and live-server tests skip without their environment. Astra's
battery stays member-only.

## D7: the verification protocol

PR 1: (1) pristine proof by git blob hashes (`git ls-files -s
vendor/engine`, prefix-stripped and sorted, equals
`git ls-tree -r <rev>`, never working-tree bytes, because autocrlf pins
LF only for some extensions); (2) full workspace battery green;
(3) xtask gate green; (4) `cargo tree` and `Cargo.lock` contain no
`allodia-eu` source lines; (5) `cargo metadata --no-deps` shows no engine
crate in `workspace_members` (cms still present, proving the exclude did
not regress it); (6) the nested workspace compiles standalone inside
`vendor/engine`; (7) the live harness suite passes serial; (8) the
`tls-native-certs` feature survives.

PR 2: the `--dry-run` prints the current and new revisions with the
classified digest; `gh auth status` passes; the external worktree exists.

PR 3: the sync PR's diff is `vendor/engine`-only; the pre-merge check
(full battery, gate, live harness, engine battery inside `vendor/engine`)
runs with `CARGO_BUILD_BUILD_DIR` pointed at a private directory (the
shared build-dir uplift hazard: worktrees whose `vendor/` diverges from
another checkout must never share the default build dir).

PR 4: the scheduled command line runs by hand and exits 0 on the no-op
path.

## Known traps recorded, not hidden

- T1: `exclude = ["vendor"]` is mandatory; without it Cargo's
  auto-membership collides with the nested workspace and nothing
  resolves.
- T2: the engine's tests do not join Astra's battery; they run inside
  `vendor/engine` on sync PRs, and upstream-sync.md says so.
- T3: the repo's shared build-dir freshness is keyed on
  workspace-relative paths; a worktree whose `vendor/` diverges must set
  `CARGO_BUILD_BUILD_DIR` to its own directory or it may be handed stale
  engine artifacts.
- T4: sync PRs merge with merge commits (see D4); squash would break the
  incremental model.
- T5: `british_english` and `dash_hygiene` need the vendor exemption too;
  `doc_sync` needs none.
- T6: CLAUDE.md has no zh mirror; only AGENTS.md and `docs/*.md` changes
  owe mirrors.
- T7: the copy is `git archive`, never robocopy; `.github/` and
  `Cargo.lock` stay as inert provenance.
- T8: pristine proof compares git blob hashes, never working-tree bytes
  (autocrlf pins LF only for some extensions).
- T9: the rusqlite coupling (one libsqlite3-sys for both stores, pinned
  0.40) becomes intra-repo: an upstream rusqlite bump now arrives through
  the sync PR and can break `astra-ai` in the same build. The digest
  classifies `Cargo.toml`/`Cargo.lock` changes as their own bucket and the
  review checklist names them.
- T10: the nightly anchor is `vendor-sync`'s own `vendor:` commit
  history (fallback: refs-lock baseline), never `last_ported` alone.
- T11: `gh` is installed but auth is verified at PR 2, not assumed.
- T12: the nightly worktree lives outside the repository
  (`D:/Projects/mailclient/Astra-vendor-sync`), so the gate never walks it.
