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

## The send gate

Every send a submit starts passes the registered `before_send` handlers
**before it reaches the engine's queue** (the engine owns the queue's
scheduling, so a veto after queueing is no veto at all), and every
composition's rendered bodies pass the registered `composer.transform`
handlers before the gate. A send a handler blocks is never queued: it stands
in the outbox surface as a held row (`SendingSnapshot.held`) naming the
plugin and the reason, backed by the draft the autosave already persists.
There is no plaintext fallback by construction.

The manifest declares the mode (`before_send_mode`, required with the
handler):

| Mode | On a violation | On an unavailable or timed-out handler |
|---|---|---|
| `block` (default) | The send holds, blocked. | The send holds (fail closed). |
| `soft_block` | The send holds, blocked. | The send proceeds (fail open). |
| `prompt_user` | A Send Anyway / Deny question the person answers. | The send proceeds (no question was ever asked). |

Timeouts follow Outlook's shape: the held row names the awaited plugin from
the first second, and a 5-minute cap ends the wait, folded by the mode.
One handler per plugin per composition; auto-activation costs the `admin`
grant, so a send-granted but non-admin plugin is never called. Send Anyway
queues without re-gating and is never remembered; Deny drops the hold and
keeps the draft; retry re-runs the gate on the held composition; withdraw
drops the hold. Transforms run in registry order, each seeing the previous
one's output, and a failed or unavailable transform holds the send closed
(the untransformed mail never goes).

## The UI contribution model

Descriptors first: the manifest's optional `[ui]` table declares static shapes
and Astra's chrome renders them. `composer_actions` become buttons in the
composer chrome, `settings_sections` become categories in the settings
screen's Plugins detail (each field in its kind's control: toggle, text,
select, password, action), and `pages` open inside the sandboxed frame. A
click or a field change is a `plugin_call` into the worker's declared custom
method (`plugin.<id>.<name>`), answered with the worker's own payload; an
action's effect shows through the answer (for example a toggle's `pressed`),
because a worker never touches the draft directly.

The reading pane's banner slot (`PluginBanner`) renders the decode answer's
`security_banner` leg: the plugin's level and words, mounted above the body.

Rich pages run in the `PluginFrame`: an iframe over the `astra-plugin://`
scheme, which resolves `astra-plugin://<plugin-id>/<path>` inside that
plugin's own directory only (traversal and cross-plugin reads refused,
missing files 404, HTML with a CSP of `default-src 'none'` plus the bundle's
own scripts/styles/images/fonts). The page talks to its worker over a narrow
postMessage bridge (`frame_call` / `frame_result` / `frame_error`), served
only from the frame's own window and gated by the same declared-method rule
as every call.

## Distribution

A `.astra-plugin` is an uncompressed tar of the plugin's directory plus
`publisher.pub` and `signature.bin`: the signature is ed25519 over the
canonical digest (the sha256 of the sorted `(path, sha256(content))` pairs,
the signature itself excluded, the publisher key included). Verification is
tamper-evidence plus identity: the signature must verify against the bundle's
publisher key, and that key must equal the manifest's `publisher` field. The
fingerprint the consent dialog shows is the sha256 of the publisher key,
first eight bytes, hex. This is deliberately not a certificate chain: the
trust decision is the human's at install.

Production builds (no `ASTRA_PLUGIN_DEV`, a release) refuse unsigned or
unverifiable bundles; dev mode accepts unsigned directory trees as before.
`cargo run -p xtask -- bundle-keygen <key-file>` mints a publisher key and
prints the manifest's `publisher` field; `bundle-plugin <dir> <key-file>`
packs and signs.

Install is offer-then-consent: `plugin_install_offer` verifies and names
everything (id, name, version, the publisher fingerprint, the requested
permissions, the privacy policy URL), the consent dialog's checkboxes start
unchecked (fail closed), and `plugin_install` re-verifies, unpacks into the
managed directory, and records exactly the checked grants. Installed plugins
land disabled until enabled by hand.

Grants and the enabled flag write through to `grants.toml` beside the
registry root; the boot reads them and re-enables the enabled set (a worker
that now fails lands Failed, never a failed boot). A production build never
enables a plugin declaring a proposed method. The `plugins_enabled`
preference (the settings snapshot's block, default on) is the admin policy
switch: off means the boot enables nothing, `enable` refuses, and the gate,
decoder, and call surfaces answer as if no plugin were serving.

## Track B: the built-in crypto

Message encryption is a product feature, not a plugin: S/MIME and OpenPGP
ship inside the app core (the 2026-10-20 decision), so no plugin ever sees
plaintext it does not need and the trust model of the send gate is the
app's own. What stands after SP4b:

- **The store** (`crates/astra-crypto-store`): one `crypto.sqlite` beside
  the AI store, kylins' schema in its final form (`crypto_keys` incl.
  harvested/intermediate, append-only `trust_decisions`, `collected_keys`,
  `contact_pinned_keys`, `crl_cache`, `message_crypto_results`,
  `send_as_aliases`). Soft private blobs wrap at rest under an AAD-bound
  AES-256-GCM master key in the OS keyring. The `SqliteKeyStore` bridge
  implements the core `KeyStore` (account-bound, the `standard|fingerprint`
  KeyId, the OpenPGP subkey fallback, token handles, the fetchability
  filter); the trust decisions and the recipient resolution ladder (NoKey
  with the collected hint, expiry first, own key Ready, the
  verified/personal/unverified Ready model, Rejected, Undecided,
  AcceptedOtherEmail, pin-preferred) mirror kylins' `resolve_recipient_status_at`.
- **The send gate** (`crates/astra-crypto-core::apply`): kylins'
  `apply_crypto` semantics, verbatim. Passthrough, `NoSigningKey`, the
  fail-closed trust gate (every recipient must resolve Ready; no plaintext
  fallback, no ciphertext on refusal), pin-aware selection so gate and
  encrypt use the same key, the S/MIME-only chain gate against the stored
  anchors, protected-headers subject hiding (default on, fail-safe to on),
  OpenPGP Autocrypt-Gossip inside the encrypted part, `EncryptOp.sign_with`
  composition (never clear-sign-then-encrypt), and the W9 password arm
  (SKESK symmetric, `PasswordConflict`/`PasswordRequired`).
- **The Astra-side outbox** (spec D3): the pinned engine's submit takes
  only `Draft`, so a crypto send assembles with the engine's own
  `engine_rfc5322` (byte-identical output), wraps once (the filed variant,
  with `Bcc`), and queues in `crypto_outbox`. The wire variant is the same
  ciphertext minus the `Bcc` header. The bypass transport is small and
  explicit: EHLO, `AUTH PLAIN`, `MAIL FROM`, `RCPT TO`, dot-stuffed `DATA`,
  `RSET` on error, over the account's dialled settings and the host-built
  TLS connector; the Sent copy files by IMAP `APPEND` (special-use `\Sent`
  discovery). A mid-DATA loss lands `unconfirmed`; only the user's answer
  re-sends. Crypto rows ride the existing sending snapshot under a high-bit
  op namespace with the same verbs routed.
- **The switch**: each account's `[crypto] method` in its stored config
  (`none | openpgp | smime`), echoed on the settings row and editable from
  the Accounts page. `none` is the zero-cost default: the engine's queue
  keeps every send. The composer's sign/encrypt flags ride the submit
  intents, defaulting to both on under a method. A method without IMAP/SMTP
  submission settings holds with the typed unavailable answer, never a
  silent drop.

Later slices: SP4c is the receive half (decrypt/verify through the reading
pipeline, encrypted drafts at rest, the result rows and the security
banner, CRL fetching, the Autocrypt consume half); SP4d is the token
registry (cryptoki, openpgp-card) and the native security UI. A token key
with no registered provider already fails typed (`TokenNotPresent`), never
falls back to software.

## Known gaps, recorded

- Ops queued before a `before_send` handler activates are not retro-gated;
  per-op holds inside a drain need engine support the pinned revision does
  not have.
- Held rows are session state; the draft behind them is durable, and the
  held entry itself rebuilds from a resend.
- Plugin-initiated calls (a worker reading the AOM on its own) have no
  envelope yet; every call the send path needs is host-initiated, so the gap
  costs nothing today and lands when a consumer needs it.
- Routed calls are forwarded to the worker but no host intent executes them
  yet; the stable method wiring is a later slice's core work.
- Event streams exist as envelope types and subscription bookkeeping only;
  no astra-app event feeds them yet.
- The blob cache's quota and eviction are named in the contract; the cache
  itself lands with the first payload that needs it.
- The decode seam is live in the reading pipeline (the `MessageDecoder` port,
  the raw source handed to the serving plugin, decoded legs through the
  identical sanitise pipeline, the banner in the reading snapshot); what is
  not there yet is a shipped plugin that decodes anything (the crypto module
  is built-in, Track B).
- The publisher key is self-asserted (sideload trust); a curated store with
  publisher identity verification, key rotation, revocation, and auto-update
  is the plan's "later".
