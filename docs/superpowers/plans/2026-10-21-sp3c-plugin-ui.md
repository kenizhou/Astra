# SP3c plan: the decode seam and the plugin UI surfaces

Spec: docs/superpowers/specs/2026-10-21-sp3c-plugin-ui-design.md. Branch
`sp3c-plugin-ui`. Each task: reds first, the implementation, the battery, a
conventional commit.

### Task 1: the decode seam (D1, D6's decode half)

**Files:** crates/astra-viewmodel (the `SecurityBannerView` leg on
ReadingSnapshot, wire pinned); crates/astra-app (the `MessageDecoder` port,
the decode call in render_reading between the source read and the render,
decoded HTML through the same sanitise pipeline); crates/astra-plugin
(`message.decode` stays ReadItem + a message handle); plugins/echo (the
decode answer); client/desktop (the `PluginDecoder` over the registry with
the gate's timeout, installed at boot, and the echo-over-duplex decode test).

- [ ] Tests first; commit `feat: decode messages through the plugin seam`.

### Task 2: the descriptors and the call command (D2, D3)

**Files:** crates/astra-plugin (the manifest's `[ui]` table, strict-parsed);
client/desktop (PluginRow carries the descriptors; the `plugin_call` and
`plugin_frame_call` commands through the router); client/web's
services/plugins.ts (the pulls and calls).

- [ ] Tests first; commit `feat: declare and route the plugin descriptors`.

### Task 3: the chrome renderers and the held rows (D2, D5)

**Files:** client/web (PluginBanner in the reading pane; PluginComposerAction
in ComposerActionsRow and ComposeRibbon; PluginSettingsSection as a settings
category; OutboxQueue's held rows with answer/retry/withdraw; the ipc types
and intent constructors).

- [ ] Tests first; commit `feat: render the plugin surfaces in the chrome`.

### Task 4: the frame and the scheme (D4)

**Files:** client/desktop (the astra-plugin:// scheme handler with its
per-plugin root, traversal refusal, and CSP); client/web (PluginFrame +
pluginBridge.ts); tauri.conf.json's CSP naming the scheme.

- [ ] Tests first; commit `feat: frame the plugin's rich pages behind the scheme`.

### Task 5: the docs and close-out

**Files:** docs/plugins.md + zh (the UI model section); capabilities rows +
zh; the changelog fragment; full battery; push; PR.

- [ ] Commit `docs: record the plugin UI slice`; then close out.

## Self-review notes

- Spec coverage: decode (T1), descriptors and calls (T2), chrome (T3), frame
  (T4), docs (T5). Every D lands in exactly one task.
- Interface chain: the snapshot leg and port (T1) -> the manifest and command
  (T2) -> the renderers consume both (T3) -> the frame extends the call
  surface (T4).
- Risks: decoded HTML must take the identical sanitise pipeline (no second
  body path); the scheme handler must never resolve outside the plugin's own
  directory (`..`, absolute, and cross-plugin ids all refused); the bridge
  must refuse undeclared methods before the pipe; the held rows must follow
  the snapshot, never local state that could disagree with it.
