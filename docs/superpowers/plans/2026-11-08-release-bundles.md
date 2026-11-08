# Release bundles plan: installers on the release

Spec: docs/superpowers/specs/2026-11-08-release-bundles-design.md. Branch
`release-bundles` from main at the docs commit. One task: reds where they
apply, the implementation, the battery, a conventional commit.

### Task 1: the config fix and the workflow

**Files:** client/desktop/tauri.conf.json (the duplicate beforeDevCommand and
beforeBuildCommand keys collapse to one canonical pair, the staging chain
live); .github/workflows/release.yml (the three-OS matrix, the explicit chain,
the per-OS bundle lists, tag-mode attach, dispatch-mode artifacts, the tag
input); the changelog fragment.

- [ ] The effective-config read pins the fix (one pair, the staging chain
      live) before the edit.
- [ ] The workflow's dispatch run on the branch is the verification: three
      OSes green, the installer names in the artifacts.
- [ ] Commit `feat: bundle the installers onto the release`.

### Task 2: close-out

- [ ] Full battery; branch pushed; PR (squash expected); after the merge, the
      tag-input dispatch attaches v0.2.0's assets to the standing release.

## Self-review notes

- Spec coverage: the config fix and the workflow are one task; the close-out
  attaches the standing release's assets.
- Risks: the workflow cannot be fully verified before the merge (the dispatch
  run on the branch is the verification, and the tag-mode attach is verified
  on the standing release after); the AppImage leg may flake on Ubuntu (a
  broken installer fails loudly, never ships); unsigned is the named
  remainder with the caveat in the release body.
