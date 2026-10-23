# Engine vendoring plan: vendor/engine/ and the vendor-sync flow

Spec: docs/superpowers/specs/2026-10-23-engine-vendoring-design.md.
Branch `engine-vendoring` from the SP4b tip. Each PR: reds first where
behaviour changes (the xtask exemption fixtures), the battery, the gate,
a conventional commit.

### PR 1: the vendor switch at 68eab00

**Files (new):** `vendor/engine/**` (`git -C <upstream> archive
68eab00e6a9a9fe5a200218e156dbaae1ba00caa | tar -x -C vendor/engine`);
`docs/changelog/unreleased/engine-vendoring.md` (Platforms/Bump tags).

**Files (modified):** `Cargo.toml` (`exclude = ["vendor"]` in
`[workspace]`; ten git lines become ten path lines, features preserved;
section comment repointed at docs/upstream-sync.md); the four xtask
checks (`license_headers.rs`, `file_length.rs`, `british_english.rs`,
`dash_hygiene.rs`, each gaining the `vendor/` exemption and a fixture
test, optionally sharing a `VENDORED_ROOT` constant in `checks/mod.rs`);
`.cargo/config.toml` (the commented `[patch]` block out, header comment
rewritten); `Cargo.lock` (regenerated); `AGENTS.md` + zh mirror;
`CLAUDE.md`; `docs/upstream-sync.md` + zh mirror (the rewritten Engine
updates section: vendored model, vendor-sync branch, merge-commit rule,
last_ported advance, engine battery inside `vendor/engine`,
CARGO_BUILD_BUILD_DIR rule).

**Commit:** body carries the literal line `vendor: upstream
68eab00e6a9a9fe5a200218e156dbaae1ba00caa initial import` (the nightly
anchor format).

- [ ] Verification D7 (all eight checks); push; PR `engine: vendor the sync engine at 68eab00`.

### PR 2: the sync tooling and the dry-run

**Files:** `scripts/dev/upstream-sync.sh` (idempotent, `--dry-run`, the
digest classifier, gh PR create-or-update); `docs/upstream-sync.md` + zh
(the scheduling section and the conflict runbook). Branch and worktree:
`vendor-sync` from main, pushed; `git worktree add
D:/Projects/mailclient/Astra-vendor-sync vendor-sync`.

- [ ] `--dry-run` prints CUR=68eab00, NEW=73650cc and the digest;
  `gh auth status` passes. Commit `feat: the engine upstream-sync driver`.

### PR 3: the first real sync (68eab00 -> 73650cc)

**Files:** the script's own output (the `vendor-sync -> main` PR);
`refs/refs-lock.toml` (`last_ported` advance); a changelog fragment for
the upstream behaviour changes; any Astra-side adaptation the 13 upstream
commits require, as ordinary commits on main.

- [ ] Sync PR diff is `vendor/engine`-only; pre-merge battery + gate +
  harness + engine battery (private CARGO_BUILD_BUILD_DIR); merge with a
  merge commit; advance `last_ported`.

### PR 4: the scheduled job

**Command:** `schtasks /Create /F /TN "Astra\vendor-upstream-sync"
/SC DAILY /ST 22:00 /TR "\"C:\Program Files\Git\bin\bash.exe\" -lc
'D:/Projects/mailclient/Astra/scripts/dev/upstream-sync.sh >>
D:/Projects/mailclient/logs/vendor-sync.log 2>&1'"` (log outside the
repo).

- [ ] `schtasks /Query` shows it; the exact scheduled command line runs
  by hand and exits 0 on the no-op path.

## Self-review notes

- Spec coverage: every D lands in exactly one PR (D1-D3 -> PR 1, D4/D5 ->
  PR 2, D4's first execution -> PR 3, the schedule -> PR 4, D6/D7 -> the
  verification steps of each).
- Interface chain: the vendored tree (PR 1) is what the script drives
  (PR 2), the first merge proves the loop (PR 3), the job keeps it alive
  (PR 4).
- Risks: the `exclude` line lands in the same commit as the path lines
  (never one without the other); the import commit carries the anchor
  format; sync PRs merge with merge commits; the pristine proof compares
  blob hashes, not bytes.
