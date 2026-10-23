# SP4b-seam design: the crypto gate moves into the engine (bypass retired)

**Status.** Decided 2026-10-23. Branch `engine-crypto-seam` from main. This
spec **supersedes the bypass half of
`2026-10-22-sp4b-crypto-send-design.md`'s D3** (left standing with a banner;
history is not rewritten). User decisions 2026-10-23: keep the architecture
simple: modify the ENGINE, not Astra (their note: "现在的方向已经确定了，我
们应该保持架构的简洁性，应该重新修改engine 而不是 Astra"), and of the
three shapes put to them they picked: engine failed row + Send Now for
trust-gate refusals; wrap the filed variant once and strip Bcc for the wire;
IMAP first with a provider-neutral trait.

**Why the reversal.** SP4b's D3 shipped a second, much smaller send engine:
the Astra-side `crypto_outbox` and its own SMTP submitter + IMAP Sent
filing, because the then-pinned engine accepted only `Draft` payloads and
could not be touched. The vendoring slice removed exactly that constraint : 
the engine now lives in-tree at `vendor/engine/` and in-tree edits are the
supported flow. The bypass's own known gap (a second send engine that must
re-earn retry/backoff, unconfirmed semantics, partial-DATA handling, and
Sent-copy discipline the engine's submitter already hardened) is hereby
retired: one send engine, one outbox, one drain.

**Scope.** The engine's MIME-wrap seam (engine-provider trait, provider-imap
consumer), the Astra wrapper over `apply_crypto`, the dial-time installation,
and the removal of the Astra bypass (the crypto outbox table, the Astra
transport, the submit-tail branch, the crypto rows and verbs). The crypto
store (keys/trust/resolution), the gate itself, the crypto-method setting,
and the composer's flags are untouched in shape.

## D1: `MimeWrapper` in engine-provider, consulted per draft

A new provider-neutral trait beside `hand_over.rs` (the `ConnectObserver`
precedent: neutral trait, Arc-carried through config):

```rust
pub struct MimeWrapOutcome { pub wire: Vec<u8>, pub filed: Vec<u8> }

#[async_trait]
pub trait MimeWrapper: Send + Sync {
    async fn wrap(
        &self,
        draft: &Draft,
        filed_message: Vec<u8>,
    ) -> Result<MimeWrapOutcome, ProviderError>;
}
```

The engine consults it only when a wrapper is installed AND the draft asks
(`draft.sign || draft.encrypt`); every other send is byte-identical to
today. The wrapper receives the FILED assembly (with the `Bcc` header) and
returns both variants: `filed` is the wrapped message, `wire` is the same
ciphertext with the `Bcc` header stripped (one wrap per send). A refusal is
`ProviderError::permanent(detail)`: it happens before any socket dials and
before `hand_over.commit()`, so a refused send never reaches the server, the
op settles `Failed` with the reason, and the engine's existing Send Now
(`retry_pending_op_now`) re-assembles and re-wraps. `Draft` gains
`#[serde(default)] pub sign: bool` and `pub encrypt: bool` (the type's own
established pattern; payloads serialized before the change deserialize with
both `false`). Graph and Google assemble too and can adopt the same trait
later; JMAP sends `Email/set` objects and is out of scope.

## D2: provider-imap consumes the seam before dialling

`ImapConfig` gains `mime_wrapper: Option<Arc<dyn MimeWrapper>>` and a
`with_mime_wrapper` builder (modelled on `with_connect_observer`), threaded
along the exact path `smtp` already rides (`ImapConfig -> ImapAccount::connect
-> build -> ImapProvider`). Submission preparation moves to a shared
`prepare_message(draft, now, wrapper)` helper (new `wrap.rs`, because
`filing.rs` is at the file-length budget): wrapper installed and a flag set
-> assemble the FILED variant, `wrapper.wrap`, carry both byte strings in
the `Submission`; otherwise `assemble_message`, today's behaviour
byte-for-byte. Preparation is hoisted **above** the transport match, so a
refusal is pre-dial on implicit TLS, STARTTLS, and plaintext alike. The
filed-copy selection prefers the wrapped output; today's no-Bcc fast path
and filed re-assembly survive only for unwrapped sends. `refile()` (the
`file_sent_copy` repair) runs the same helper, so an `Unfiled` crypto send
can never re-file a plaintext Sent copy: the single worst leak this change
must not ship.

## D3: the Astra wrapper in astra-crypto-store

`StoreMimeWrapper` (new `src/wrap.rs`) implements the trait over the ported
gate: it registers the account row idempotently, builds the gate's
`SendDraft` from the engine `Draft` (recipients + flags), resolves the
default signing key, picks the backend by the bound standard, and calls
`apply_crypto` on the filed assembly. The outcome is `{ wire:
strip_bcc(wrapped), filed: wrapped }`; a `CryptoSendError` becomes
`ProviderError::permanent(gate_failure_words(...))`. `strip_bcc`,
`gate_failure_words`, and `address_spec` move here from astra-app verbatim
with their tests. The factory `mime_wrapper_for(store, account_id,
account_email, method)` answers `None` for method `none`, and an
**always-refusing** wrapper when the store cannot serve (fail-closed, never
plaintext). The `crypto_outbox` table drops in migration 0003 (0002 stands
as immutable history; SP4b is unreleased, so the discarded queue is safe
and the changelog says so). `src/outbox.rs` and `src/transport.rs` are
deleted with their tests.

## D4: installation at dial, re-dial on toggle

`connect_mail_providers` (and `connect_imap_mailbox` for late folder binds)
gain an optional wrapper argument; the desktop host builds the wrapper per
account whose `[crypto]` section names a method and passes it in, with the
crypto store opened BEFORE `dial_stored` at boot. `store_crypto_method`
re-dials before persisting and re-installs the providers in place: the
previous "no re-dial" contract inverts, because the wrapper is bound at
dial. A method written for an account without `[imap]` + `[smtp]` is
refused host-side (`encryption needs an IMAP/SMTP account`): a method with
no seam to attach a wrapper would otherwise send plaintext, which is the
fail-closed regression this slice refuses to ship. The settings row's
`crypto_method` echo and the Accounts-page select are unchanged.

## D5: the Astra tail shrinks to a flag stamp

`submit_and_drain` stamps `draft.with_crypto(sign, encrypt)` (resolved from
the composer's `CryptoFlags`, default both on under a method: inert when
no wrapper is installed, never defaulting to false on a crypto account) and
takes the engine tail unchanged. `crypto_submit_maybe`, the
`CryptoSendSupport` port, the crypto outbox rows and verbs, and the
`CRYPTO_OP_NAMESPACE` are deleted; held rows stay exclusive to the plugin
gate. The plugin gate keeps vetting the composition before the flag stamp,
so it sees exactly what the wrapper will wrap, and held entries keep their
flags for Send Anyway/Retry.

## D6: refusal UX is the engine's own

A trust-gate refusal is an engine `Failed` row with the per-recipient
reason (the queued row derives subject from the draft payload for free),
and Send Now retries after the user accepts the key. Nothing else changes:
transient failures retry with backoff, mid-DATA loss stays `unconfirmed`
with the confirmation pair, and the Sent copy is filed by the engine's own
APPEND path: every semantic the bypass re-implemented at half size.

## Known traps recorded, not hidden

- The flags must be `#[serde(default)]`: an op queued by an SP4b build
  would otherwise fail to decode at drain (tested with a literal old-shape
  JSON).
- The no-Bcc fast path in `file_result` must be unreachable once wrapped;
  the filed copy is always the wrapper's output.
- Preparation must hoist above the transport match (the plaintext arm
  dialled first); a refusal is pre-dial and pre-commit by construction.
- `refile()` re-wraps; an `Unfiled` crypto send repaired later files the
  wrapped copy or fails permanent, never plaintext.
- Engine file-length CI (submit.rs at 498, filing.rs at 483) forces a
  split before any line is added.
- Boot opens the crypto store before dialling; late folder binds thread
  the wrapper through `DialConnector`; a wrapper that cannot build at dial
  attaches as always-refusing.
- Non-IMAP accounts refuse the method write host-side; legacy stored
  configs that already carry one are logged at boot.
- The engine's own gate (nightly-pinned fmt, clippy `-D warnings`,
  file-length) runs in `vendor/engine`, and the provider change is proven
  against the live Stalwart harness, not only offline mocks.
