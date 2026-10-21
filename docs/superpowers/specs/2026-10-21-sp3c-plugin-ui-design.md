# SP3c design: the decode seam and the plugin UI surfaces

**Status.** Decided 2026-10-21. Branch `sp3c-plugin-ui` from main (SP3a
merged as #31; SP3b is #32 in flight; the branch carries whichever of it is
unmerged). Source: the approved plugin plan's Track A phase 3, plus
`message.decode` (moved here from SP3b with the banner it feeds) and the
outbox held-rows UI (the SP3b surface debt: the snapshot leg shipped without
its renderer).

**Scope.** Four surfaces land: the reading pipeline's decode seam with its
security banner; the manifest-declared UI descriptors (banner slot, composer
actions, settings sections) rendered by Astra chrome; the sandboxed
PluginFrame for rich pages over the `astra-plugin://` scheme with its typed
bridge; and the outbox panel's held-send rows with their answer, retry, and
withdraw. What does not ride: signed bundles and the install dialog (SP3d),
event streams fed by astra-app, and the plugin-to-host call direction (still
no consumer; see D3).

## D1: decode runs in render_reading, fails open, and its HTML is sanitised like any other

When an activated plugin serves `message.decode`, the reading pipeline reads
the raw source (`engine.message_source`) after resolve and hands it to the
decoder port (`MessageDecoder`, the desktop's `PluginDecoder` over the
registry, with the same per-call timeout the gate uses). The outcome's parts
replace the rendered legs: a decoded HTML leg goes through the identical
sanitise / inline-cid / remote-gate pipeline every other body takes, and a
decoded plain leg is the text view. The outcome's banner rides the reading
snapshot's new `security_banner` leg (a viewmodel type: level info/warning/
error plus text; the plugin crate's `SecurityBanner` maps at the host seam).
No decoder, an error, or a timeout: today's rendering, no banner. The decode
sees raw bytes only under the plugin's `read_item` grant, and the host mints
the call's message handle; a plugin that answers nothing changes nothing.

## D2: descriptors are manifest-declared and surface with the plugin row

The manifest gains an optional `[ui]` table, strict-parsed like the rest:
`composer_actions` (`id`, `label`, `icon`), `settings_sections` (`id`,
`title`, fields of `toggle` / `text` / `select` / `password` / `action`),
and `pages` (`id`, `title`, `entry`). The desktop's `PluginRow` carries them
to the web, which renders them with Astra chrome: a composer action is a
button in `ComposerActionsRow` / `ComposeRibbon`, a settings section a
category in the settings screen, a banner slot the reading pane's
`PluginBanner` (its content is the decode banner or a plugin event, never
static manifest text).

## D3: descriptor answers are custom-namespace calls, fire-and-acknowledge

A composer action click or a settings field change is a `plugin_call`
command: the host calls the worker's declared custom method
(`plugin.<id>.<action>`, `plugin.<id>.set_setting`) through the router
(declared + granted like any call) and answers the plugin's result to the
webview. The result may carry fresh descriptor state (for example the
Encrypt toggle's `pressed`), which is how an action shows its effect: the
plugin-to-host direction (a worker editing the draft mid-call) stays the
recorded gap until a consumer needs it, and this slice's actions are honest
about being one-directional.

## D4: rich pages live in the PluginFrame over astra-plugin://, bridged narrow

The desktop registers the `astra-plugin` URI scheme: a request for
`astra-plugin://<plugin-id>/<path>` resolves inside that plugin's own
directory only (traversal and cross-plugin reads refused, missing files 404,
HTML served with a CSP that allows no remote anything). The web's
`PluginFrame` renders the page in a sandboxed iframe and bridges
`postMessage` envelopes (`{ id, method, args }` mirroring
`object_call`/`object_result`) through a new `plugin_frame_call` command to
the worker and back. No plugin JS ever runs in the main webview; the frame
is the whole of its reach.

## D5: the outbox panel renders the held leg

`OutboxQueue` gains the held rows below the queue: `checking` shows the
plugin being awaited, `needs_answer` shows the reason with Send Anyway /
Deny (dispatching `AnswerGate`), `blocked` shows the reason with Retry /
Withdraw (`RetryHeld` / `WithdrawHeld`). The web ipc layer gains the held
types and the three intent constructors.

## D6: the proofs

Unit reds pin: the decode seam (parts replace, HTML sanitised, banner rides,
absent decoder costs nothing, errors fail open), the manifest's `[ui]` parse
(strict, optional), the scheme handler (traversal refused, cross-plugin
refused, missing 404, CSP present), and the bridge envelope (unknown method
refused before the pipe). vitest pins each renderer (banner levels, action
click dispatching, settings field kinds, held rows' three states). The echo
plugin gains a `message.decode` answer (the raw echoed as a plain part, no
banner) and the desktop decode test drives it over duplex. The reading
pipeline's live proof is the existing harness reading suite, which the
decode seam leaves byte-identical when no decoder is installed.

## Known gaps recorded, not hidden

- The plugin-to-host call direction (a worker initiating reads or edits) has
  no envelope yet; this slice's surfaces are all host-initiated, and the
  composer actions acknowledge rather than edit.
- Grant persistence, the install/consent dialog, and signed bundles are
  SP3d; until then plugins enter through `ASTRA_PLUGIN_DEV` only.
- `ui.banner`'s runtime content is the decode banner; event-driven banners
  (plugin_event) land with the event streams.
- The frame's bridge allows the declared custom methods only; the fixed
  object tree (reads, edits) arrives with the plugin-to-host direction.
