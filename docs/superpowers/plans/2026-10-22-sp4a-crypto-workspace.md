# SP4a plan: the crypto workspace, ported

Spec: docs/superpowers/specs/2026-10-22-sp4a-crypto-workspace-design.md.
Branch `sp4a-crypto-core` from main. Each task: the mechanical port first,
then the battery, a conventional commit per crate family.

### Task 1: the workspace skeleton and the core crate (D1, D3, D4)

**Files:** crates/astra-crypto-core/ (byte-identical sources, renamed package
and intra-deps, the crate-doc rename); the xtask file-length exemption list;
Cargo.lock refresh.

- [ ] `cargo test -p astra-crypto-core` green; commit
  `feat: vendor the crypto core crate from kylins`.

### Task 2: OpenPGP (D1, D4, D5)

**Files:** crates/astra-crypto-openpgp/ (sources + tests, sequoia flags,
the core path dep renamed).

- [ ] lib + round_trip + interop (skipping without gpg/sq) green; commit
  `feat: vendor the openpgp crypto crate from kylins`.

### Task 3: S/MIME with published cms (D1, D2, D4, D5)

**Files:** crates/astra-crypto-smime/ (sources + tests; `cms = 0.3.0-rc.0`
replacing the path dep; the dual-stack aliases; the profile renames).

- [ ] lib (incl. openssl interop where the CLI exists) green; commit
  `feat: vendor the smime crypto crate from kylins`.

### Task 4: the repo-law pass and the record

**Files:** fmt/clippy/dash-hygiene adjustments (recorded deviations);
docs/plugins.md's Track B note + zh; the changelog fragment; capabilities
row + zh.

- [ ] Full gate green; commit `docs: record the crypto workspace port`.

### Task 5: close-out

- [ ] Full battery (`cargo test --workspace`, web untouched); branch pushed;
  PR.

## Self-review notes

- Spec coverage: core (T1), openpgp (T2), smime + cms swap (T3), repo law +
  record (T4). Every D lands in exactly one task.
- Interface chain: the exemption (T1) -> the crates land (T1-T3) -> the gate
  accepts them (T4).
- Risks: the cms swap is the only non-mechanical change; if any kylins test
  fails against 0.3.0-rc.0, the vendored copy returns and D2 records the
  failing call site. The exemption must name directories, never glob the
  whole tree. The openpgp lib battery is slow in debug (~6 min upstream) and
  must not be mistaken for a hang.
