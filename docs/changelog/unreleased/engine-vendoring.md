# The sync engine moves in-tree (vendor/engine/)

Platforms: desktop
Bump: none

- The email-calendar-sync-engine is now vendored at `vendor/engine/`: the
  full upstream tree (all 22 crates, MPL-2.0) at the revision
  `refs/refs-lock.toml` records, consumed through the same `engine-api`
  facade with the same features as the previous git pins. Builds are
  behaviour-identical to before; what changes is that engine modifications
  can now land in-tree as ordinary commits, and upstream changes arrive
  through the nightly vendor-sync flow (a `vendor-sync` branch holding the
  pristine copy, each sync a merge PR; see docs/upstream-sync.md).
