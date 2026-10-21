# SP3d design: plugin distribution

**Status.** Decided 2026-10-21. Branch `sp3d-plugin-distribution` from main
(SP3a/b/c merged). Source: the approved plugin plan's Track A phase 4, plus
the two debts the earlier slices recorded: grant persistence and the
production refusal of proposed-method plugins.

**Scope.** The signed bundle and its verification, the install and consent
flow, grants and enabled state persisted across restarts, the admin policy
switch, and the proposed-method refusal outside dev builds. What does not
ride: the curated store (the plan's "later"), key rotation and revocation
infrastructure, and auto-update.

## D1: the bundle is a tar, signed ed25519, self-identifying

A `.astra-plugin` is an uncompressed tar of the plugin's directory:
`manifest.toml`, the worker binary, any frame assets, plus `publisher.pub`
(the publisher's ed25519 public key, hex in the manifest's new `publisher`
field too) and `signature.bin`. The signature covers the canonical digest:
the sha256 of the sorted `(path, sha256(bytes))` pairs of every entry except
`signature.bin` itself. Verification is tamper-evidence plus identity: the
signature must verify against the publisher key the manifest names, so a
bundle rewritten in flight answers refusal, and the publisher's fingerprint
is what the consent dialog shows. This is deliberately not a certificate
chain: the trust decision is the human's at install, the same shape as
sideloading anywhere, and the record says so.

## D2: production verifies, dev mode sideloads

Outside dev mode (`ASTRA_PLUGIN_DEV` unset *and* a release build), install
refuses an unsigned or unverifiable bundle. Dev mode keeps today's flow: a
directory tree under the managed plugins dir or an `ASTRA_PLUGIN_DEV` path
loads unsigned. `cargo run -p xtask -- bundle-plugin <dir> <key>` builds and
signs a bundle, so the tooling is the repo's own.

## D3: install is offer-then-consent

The Plugins settings page gains "Install from file": the host verifies and
parses the bundle and answers an offer (id, name, version, the publisher
fingerprint, the requested permissions, the privacy policy URL). The consent
dialog lists the requested permissions with a checkbox each (default
unchecked, fail closed), the publisher and the policy link, and Install or
Cancel. Install extracts the bundle into the managed plugins directory,
records the checked grants, and leaves the plugin disabled until the person
enables it. `plugin_install_offer` and `plugin_install` are the two
commands; the offer computes nothing the install does not re-verify.

## D4: grants and enabled state persist

The registry writes `grants.toml` beside the managed plugins directory on
every approve and install: per plugin, the granted set and whether it is
enabled. Boot reads it: grants land before any worker starts, and plugins
recorded enabled are enabled at boot (a failure lands them Failed, never a
failed boot). The registry's in-memory table is the write-through cache;
the file is the truth across restarts.

## D5: the admin policy switch

A persisted `plugins_enabled` preference (default on) rides the settings
snapshot's new plugins block. Off means: boot enables nothing, `enable` is
refused, and the gate, decoder, and call surfaces answer as if no plugin
were serving (the registry's serving/call paths consult the switch). The
Plugins settings page holds the toggle; the registry and the surfaces follow
it live.

## D6: production refuses proposed-method plugins

The router already refuses proposed calls outside dev builds. Distribution
adds the load-time refusal: a production build will not enable a plugin
whose manifest declares any proposed method, so a release never runs a
placeholder API. Dev builds route them as today.

## D7: the proofs

Unit reds pin: the bundle round trip (build, sign, verify, extract), the
tamper refusals (a flipped byte, a wrong key, a missing signature in
production), the grants file (write, reload, the enabled flag), the
proposed refusal (dev vs production), the policy switch (off refuses enable
and empties serving sets), and the consent flow (the offer names everything,
the install persists the checked grants). The web pins the consent dialog
(the permissions default unchecked, Install carries exactly the checked
set) and the Plugins page's toggle. The harness smoke stays the SP3b gate
suite, which must stay green with the registry reading its persisted file.

## Known gaps recorded, not hidden

- The publisher key is self-asserted (sideload trust); a curated store with
  publisher identity verification is the plan's "later".
- No key rotation or revocation: a leaked publisher key means a new plugin
  id until the store exists.
- The consent dialog's per-permission words are the wire names rendered
  friendly; richer copy per permission lands with the store.
- Auto-update is out; a new bundle installs over the old directory by
  replacement.
