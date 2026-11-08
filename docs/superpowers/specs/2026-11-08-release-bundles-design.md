# Release bundles design: installers on the release

**Status.** Decided 2026-11-08. Slices branch from main at this commit.

**Scope.** A tag-driven bundle workflow that builds signed-NO installers for
Windows, macOS, and Linux and attaches them to the GitHub Release, plus the
tauri config fix the workflow exposes. The release contract itself (VERSION,
the workspace version, the fragment fold) is unchanged: this slice adds the
packaging half the contract never had.

## The config fix

`client/desktop/tauri.conf.json`'s `build` object carries DUPLICATE
`beforeDevCommand` and `beforeBuildCommand` keys: the CLI-staging pair and the
web-build pair. JSON last-wins resolves them to the web pair, so the staging
chain is dead config and `cargo tauri build` on a clean tree fails at
tauri-build's externalBin check. The fix leaves ONE canonical pair:
`beforeBuildCommand` chains the web build, the CLI release build, the real
staging (the placeholder only ever bootstraps), and the sidecar's `ci` +
`build`; `beforeDevCommand` chains the placeholder staging, the sidecar build,
and the web dev server. The workflow does not depend on either: it runs every
step explicitly, the same discipline `ci.yml` already holds.

## The workflow

`.github/workflows/release.yml`, on `v*` tags and `workflow_dispatch`:

- A three-OS matrix (windows-latest, ubuntu-latest, macos-latest), the same
  runners the nightly uses, with the Linux Tauri system dependencies from the
  nightly's own list and the Rust toolchain from `rust-toolchain.toml` (the
  pinned channel's components honored).
- The explicit chain: the web dependencies (`npm ci`, the committed
  allowScripts record keeping esbuild's postinstall approved), the web build,
  the sidecar `ci` + `build`, `cargo build --release --bin astra-cli`, the
  real staging (`stage-cli.mjs` with no placeholder flag), then the bundle
  build through the tauri CLI.
- Per-OS bundle lists, the stable set: `nsis,msi` on Windows, `dmg,app` on
  macOS, `deb,appimage` on Linux. A bundle that cannot build fails the job
  (fail fast per OS; a broken installer is not a warning).
- Tag mode attaches everything the matrix built to the named GitHub Release.
  Dispatch mode (the branch's verification path) uploads the installers as
  workflow artifacts and touches no release; a `tag` input lets a later
  dispatch attach to a standing release (v0.2.0 exists with no assets and
  gains them this way).
- Unsigned, and the release body says so: Windows installers trigger
  SmartScreen and macOS Gatekeeper wants the right-click open until the owner
  provides certificates. Signing is the named remainder, the same external
  class as the Azure registration; the workflow adds no signing configuration
  and none is implied.

## Known gaps recorded, not hidden

- The installers are unsigned (no Windows code-signing certificate, no macOS
  notarisation identity); the release notes carry the caveat.
- The AppImage leg rides Ubuntu's builders; a future flake is possible and the
  per-OS job fails loudly rather than shipping a broken artifact.
- The nightly workflow is untouched (it builds and gates; it does not bundle);
  unifying the two runners is a later tidy-up, not this slice's.

## Test-first

The config fix is verified by the effective-config read (one canonical pair,
the staging chain live). The workflow's verification is the dispatch run on
the branch: all three OSes green, the artifacts present (the installer names
in the run's artifact list), and the merge's first tag-mode run attaching the
v0.2.0 assets to the standing release. A changelog fragment lands with the
workflow (English only, `Platforms: desktop`, `Bump: none`).
