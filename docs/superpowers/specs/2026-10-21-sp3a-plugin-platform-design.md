# SP3a design: the plugin platform's contracts and worker host

**Status.** Decided 2026-10-21. Source: the approved plugin plan (the
Outlook-style Astra Object Model, reviewed by /plan-ceo-review and
/plan-eng-review; crypto decided built-in, not a plugin). Branch
`sp3a-plugin-platform` from main.

**Scope.** Track A phase 1 of the approved plan: the versioned envelope
family that replaces the interim `protocol.rs`, the host-issued handle
types and their table, the router that checks every call against
manifest, grants, and handle scope, the worker host that spawns and
supervises one process per plugin, the echo stub plugin as the
conformance fixture, and the desktop host's managed plugin state with
its thin command set. The permission table widens to the reviewed seven
tiers now, because every later slice checks against it. What does not
ride: the send gate and `before_send` (SP3b), the UI descriptors and the
frame (SP3c), signed bundles and the install dialog (SP3d), and the
whole of Track B (built-in S/MIME + OpenPGP, own slice series).

## D1: the envelope family replaces protocol.rs

One JSON object plus a newline per message, `type`-tagged, snake_case,
stderr for diagnostics: the sidecar's discipline, reused verbatim. Host
to plugin: `plugin_init` (manifest, granted permissions, api_level, the
AOM version, the worker's private data_dir), `object_call` (id, handle,
method, args), `object_subscribe`, `plugin_shutdown`. Plugin to host:
`plugin_ready` (api_level, AOM version, methods served),
`object_result`, `object_error`, `object_event`, `permission_request`,
`plugin_event`. The interim `protocol.rs` (PluginInit/HookCall) is
deleted, not kept in parallel; hook_api v1's two hooks become the
method names `compose.transform` and `message.decode`, and their
request/response types stay as the payload schemas SP3b wires.

## D2: host-issued handles, never client-composed keys

Every object the plugin names is an opaque handle the host minted:
`MessageRef`, `FolderRef`, `AccountRef`, `DraftRef`, and the rest of the
tree as later slices need them. A handle binds plugin id + account +
supervisor generation and expires with the generation; the router
resolves handles against its own table and refuses anything it did not
mint. Raw engine keys (`imap:v1:u1`) never cross the boundary. The typed
refusals are distinct (`UnknownHandle`, `StaleHandle`, `ForeignPlugin`,
`OutOfScope`) so a plugin can tell a dead worker's handles from a
scope violation. This answers the eng review's forgeable-ref finding:
a `read_item` grant lets the host mint handles for the current message
only, and guessing is not a lookup.

## D3: the router checks manifest, grant, and scope, in that order

Every `object_call` passes three gates before it runs: the method is
declared in the plugin's manifest (undeclared is a typed refusal, which
is why the closed-Intent problem never arises: this envelope is not the
astra-app Intent enum); the plugin holds the permission the method
costs (the D6 table); and every handle in the call resolves inside the
plugin's scope. The method registry (`object_model/methods.rs`) names
each method's node, permission, and stability (`stable` maps to an
existing intent, `proposed` is dev-build-only until its intent lands).
The router executes nothing itself in this slice beyond the echo path:
stable method execution lands with the intent wiring of SP3b.

## D4: the worker host reuses the sidecar's supervision shape

`crates/astra-plugin/src/worker.rs` spawns one process per activated
plugin and drives the D1 envelopes over stdio. The supervision machine
mirrors the agent supervisor's: down / starting / ready / restarting /
lost, generation numbers, a 5s init timeout, a 3-restart budget with
1-4s backoff, and fail-closed draining on death: a dead worker's
pending calls error out and its subscriptions drain on the same path
its approvals would. Startup is lazy (first call, not app boot), and
`plugin_shutdown` is always followed by kill-and-reap so an
unresponsive worker cannot outlive its host.

## D5: the echo plugin is the conformance fixture

`plugins/echo/`: a tiny Rust binary that answers `plugin_ready`, echoes
every `object_call` payload back, and emits a `plugin_event` when
asked. It is the worker host's round-trip test double and later the
send gate's harness smoke stub. It ships in the repo, never in a build.

## D6: the permission table widens to the reviewed seven

`Permission` becomes `read_item`, `read_mailbox`, `compose_edit`,
`send`, `admin`, `ai.infer` (restricted is the absence of any grant,
not a variant). `compose_edit` and `send` are split now, per the
review: a composer-assist plugin must never gain the power to send.
Every permission still defaults to NeedsApproval by the exhaustive
match, and the manifest still refuses a method whose permission is not
declared. The old `transform_outgoing` / `decode_incoming` names retire
with the hook enum; the hook payload types remain for SP3b.

## D7: the desktop host holds the registry thinly

`client/desktop/src/plugin/mod.rs` is managed state: the discovered
plugins (a plugins directory plus `ASTRA_PLUGIN_DEV=1` dev paths), the
grant table, and the worker host handles. A thin `generate_handler!`
set (list / enable / disable / approve) exposes it; install-from-bundle
arrives with SP3d. No plugin ever installs or activates itself:
discovery is host-side, activation is a human decision, and the
fail-closed posture of the manifest parse is unchanged.

## Known gaps recorded, not hidden

- The router's stable-method execution is stubbed this slice; only the
  echo path round trips. The intent wiring is SP3b's core work.
- Event streams exist as envelope types and subscription bookkeeping
  only; no astra-app event feeds them yet.
- Blob references name the shape (`blob_id`, `size`, `sha256`) and the
  quota rule (256MB per plugin, LRU) but the cache itself lands with
  the first payload that needs it (SP3b's decode).
- The AOM version is a constant both sides pin; negotiated versioning
  is a later refinement, not a silent gap.
