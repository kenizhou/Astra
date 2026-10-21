# Plugins

The plugin platform's contract, decided once here and binding on every
surface. The design's full rationale (the Outlook-style object model, the
review record, and the phasing beyond this slice) lives in the SP3a spec
(`docs/superpowers/specs/2026-10-21-sp3a-plugin-platform-design.md`); this
document is the normative seam the code answers to.

## The shape

A plugin is an out-of-process worker executable plus a `manifest.toml`,
spoken to over JSONL stdio. A plugin never runs in-process, never holds an
engine handle, and never holds a DOM handle. Everything it can do is named
in its manifest, checked by the router, and approved by a human.

## The envelope

One compact JSON object plus a newline per message in each direction,
`type`-tagged, snake_case fields, stderr for diagnostics.

Host to plugin: `plugin_init` (manifest, granted permissions, `api_level`,
`aom_version`, the worker's private `data_dir`), `object_call` (`id`,
optional `handle`, `method`, `args`), `object_subscribe` (`id`, `stream`),
`plugin_shutdown`.

Plugin to host: `plugin_ready` (`api_level`, `aom_version`, the `methods`
served, which must be a subset of the manifest's), `object_result` (`id`,
`value`), `object_error` (`id`, `message`), `object_event` (`stream`,
`event`), `permission_request` (`id`, `capability`, `reason`),
`plugin_event` (`event`).

The first message after init must be `plugin_ready`; anything else, a
malformed line, or a ready after the handshake is a protocol violation and
kills the connection. Large payloads cross by blob reference
(`{ blob_id, size, sha256 }`), never by copy.

## Handles

A plugin names objects only by host-issued handles of the shape
`{kind}.{plugin}.{generation}.{serial}` (for example
`msg.dev.astra.echo.3.7`). The host mints a handle only for an object inside
the plugin's grant scope; the router resolves handles against its own table
and refuses anything it did not mint. A raw engine key (for example
`imap:v1:u1`) never crosses the boundary. Refusals are typed and distinct:
unknown (never minted, malformed, or client-composed), stale (an earlier
generation's), foreign (another plugin's), out of scope (the wrong kind for
the method). Handles expire with the supervisor generation: a restarted
worker starts with an empty table.

## The router's three gates

Every `object_call` passes, in order:

1. **Manifest**: the method is declared in the manifest. Custom
   `plugin.<id>.<name>` methods skip the registry lookup but not the
   declaration.
2. **Grant**: a registered method's permission is granted. Custom methods
   carry no registry permission and pass on declaration alone.
3. **Scope**: the call's handle is present exactly when the method expects
   one, resolves in the plugin's table, and is of the kind the method names.

Proposed methods (no backing intent yet) route in dev builds only. The
router executes nothing itself; the worker host turns a routed call into an
envelope, and later slices turn routed calls into intents.

## Permissions

Cumulative, manifest-declared, human-approved per grant. Every permission
defaults to needs-approval by an exhaustive match, so adding one forces the
table to answer for it.

| Permission | Power |
|---|---|
| `read_item` | Read the current message only; the host mints handles for that item alone. |
| `read_mailbox` | Read the whole mailbox and mutate item state (read flags, flags, archive, trash). |
| `compose_edit` | Write through the composer (document, recipients, options, transform). Never the power to send. |
| `send` | `message.send()` and the `before_send` gate. Per-use approval per composition unless pre-approved for the account. |
| `admin` | Key-management surfaces, event subscriptions, auto-activated `before_send`. |
| `ai.infer` | Host-mediated inference only: never a provider key, never a direct model call, implied by no other tier. |

`restricted` is not a permission: it is the absence of any grant.

## Supervision

One process per activated plugin, mirroring the agent sidecar's discipline:
states down / starting / ready / restarting / lost, generation numbers, a 5s
init timeout, a 3-restart budget with a linear 1-4s backoff, and fail-closed
draining on death: pending calls error out and subscriptions drain on the
same path approvals would. A spawn that cannot start, a version mismatch,
and a ready naming undeclared methods are fatal (never retried). A quit
mid-call fails the call closed. Shutdown is ask-then-kill: `plugin_shutdown`
is always followed by kill-and-reap, so an unresponsive worker cannot
outlive its host. Startup is lazy: the first start, not app boot.

## The desktop registry

The desktop host scans the managed `plugins/` directory beside the data
store, plus any directories `ASTRA_PLUGIN_DEV` names (semicolon-separated,
each pointing directly at a plugin directory). A plugin directory holds a
`manifest.toml` and the worker binary (the manifest's `entrypoint`, or the
last id segment with the OS executable suffix). Four commands drive the
registry: `plugin_list`, `plugin_enable`, `plugin_disable`,
`plugin_approve`. The grant table is session state in this slice;
persistence and the consent dialog land with the distribution slice.

## Known gaps, recorded

- Routed calls are forwarded to the worker but no host intent executes them
  yet; the stable method wiring is SP3b's core work.
- Event streams exist as envelope types and subscription bookkeeping only;
  no astra-app event feeds them yet.
- The blob cache's quota and eviction are named in the contract; the cache
  itself lands with the first payload that needs it.
- Grants are not persisted across restarts yet.
- The UI contribution model (descriptors, the sandboxed frame) is SP3c;
  signed bundles and the install dialog are SP3d.
