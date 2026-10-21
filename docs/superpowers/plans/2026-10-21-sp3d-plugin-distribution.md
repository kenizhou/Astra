# SP3d plan: plugin distribution

Spec: docs/superpowers/specs/2026-10-21-sp3d-plugin-distribution-design.md.
Branch `sp3d-plugin-distribution` from main. Each task: reds first, the
implementation, the battery, a conventional commit.

### Task 1: the bundle (D1, D2)

**Files:** crates/astra-plugin/src/bundle.rs (the tar build/read, the
canonical digest, the ed25519 sign/verify over ring, the manifest's
`publisher` and `privacy_policy_url` fields); xtask's `bundle-plugin`
command (build + sign a directory); the echo plugin's bundle fixture (built
in-test from a generated key).

- [ ] Tests first; commit `feat: sign and verify plugin bundles`.

### Task 2: persistence and the production gates (D4, D6)

**Files:** client/desktop (the registry's grants.toml write-through and
boot read, the persisted enabled flag honoured at boot, the proposed-method
refusal outside dev builds).

- [ ] Tests first; commit `feat: persist plugin grants and gate proposed methods`.

### Task 3: install, consent, and the policy switch (D2, D3, D5)

**Files:** client/desktop (`plugin_install_offer` and `plugin_install`
commands, the extraction, the policy switch consulted at enable and at the
serving/call seams); crates/astra-app (the `plugins_enabled` preference on
the settings snapshot); client/web (the Plugins page's install button and
toggle, the consent dialog).

- [ ] Tests first; commit `feat: install plugins behind consent and the policy switch`.

### Task 4: the docs and close-out (D7)

**Files:** docs/plugins.md + zh (the distribution section);
docs/capabilities.md rows + zh; the changelog fragment; full battery; push;
PR.

- [ ] Commit `docs: record the plugin distribution slice`; then close out.

## Self-review notes

- Spec coverage: bundle (T1), persistence and production gates (T2), the
  flows (T3), docs (T4). Every D lands in exactly one task.
- Interface chain: the bundle format and verification (T1) -> the registry's
  persistence (T2) -> the install and policy flows consume both (T3).
- Risks: the digest must be canonical (sorted, path-tagged, signature
  excluded) or verification is moot; extraction must never write outside
  the managed directory (the SP3c scheme handler's traversal discipline
  applies to tar entry names); a bundle whose manifest id mismatches its
  directory must refuse; boot must never fail over a plugin recorded
  enabled whose worker now fails.
