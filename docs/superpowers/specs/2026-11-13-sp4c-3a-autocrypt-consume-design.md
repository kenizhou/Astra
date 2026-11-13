# SP4c-3a design: Autocrypt consume

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** The first leg of SP4c-3: the inbound half of Autocrypt. The send
gate already emits `Autocrypt:` and `Autocrypt-Gossip:` headers; the receive
path already consumes staged collected keys as extra signer blobs. What is
missing is the consumer: parse the inbound headers and stage the keys. This
sub-slice is the most bounded of SP4c-3 (cap-exempt crypto crates only, no
network, no UI, no migration); CRL fetching (SP4c-3b) and the W9 password
flow (SP4c-3c) follow as their own slices. No engine change; no eval /
pipeline bump (the model-facing surface is unchanged).

## D1: the inbound parser

A consume module beside the emitters in `astra-crypto-store/src/autocrypt.rs`
(the module doc already names the gap):

- Unfold and read the `Autocrypt` / `Autocrypt-Gossip` header values. Split
  the attributes on `;`, extract `addr`, `prefer-encrypt` (Autocrypt only),
  and `keydata`; base64-decode `keydata` (the emitters use base64-STANDARD of
  the binary `minimal_export` blob).
- `parse_first_cert` the blob (the existing inbound primitive) → the
  fingerprint. Malformed keydata stages nothing.
- Bind the address: Autocrypt Level 1 requires the `Autocrypt` addr to match
  the From address, else discard; a gossip addr is the recipient peer.
- Stage via `stage_collected_key_if_absent` with `source='autocrypt'` (the
  dedup key is (account, standard, fingerprint, source), so a re-opened
  message does not duplicate; `promote_origin` already maps 'autocrypt').

A tiny pub core helper exposes the header value (the store cannot call the
pub(crate) `mime::header_value`; the SP4c-2 `from_email` precedent). The
attribute/keydata parsing lands in autocrypt.rs beside the emitters.
Crypto-envelope interpretation stays in Astra's crypto layer (the detection
boundary).

## D2: the receive hooks

- The cleartext `Autocrypt:` header: `receive_openpgp` already splits the
  source into (outer, entity) and discards the outer; the consume reads the
  outer headers and stages the sender's key (addr must match From).
- The `Autocrypt-Gossip:` headers live inside the encrypted part's
  protected-headers wrapper; after `unwrap_protected_headers` the consume
  reads them from the decrypted inner header block and stages the peer keys.
- Staging is silent (no UI; the review/promote surface is SP4d). The payoff
  is immediate: `receive_openpgp` already offers the collected certs to
  `decrypt_with_signers` and `resolve_signer`, so a staged key improves the
  next message's embedded-signature verification.

## D3: the contract

docs/crypto-receive.md gains the Autocrypt-consume note (the inbound parse,
the addr binding, the silent staging, the immediate verification payoff)
and mirrors it to zh. The capabilities OpenPGP row's note names the consume
half landing. A changelog fragment lands. The porting-backlog is unaffected
(this is a C-cluster partial row, not a kylins addition).

## Known gaps recorded, not hidden

- The review/promote UI for collected keys is SP4d; staging is silent here.
- Gossip consume reads the protected-headers wrapper the send gate emits;
  third-party gossip placements beyond that wrapper are a follow-up.
- CRL fetching (SP4c-3b) and the W9 password flow (SP4c-3c) are separate
  slices; the W9 send side is itself unwired (a coordination note for 3c).

## Test-first

Every behaviour lands with its red. The parser tests (the emit→consume
round-trip: emit an Autocrypt header, consume it, the staged key matches;
the addr≠From discard; malformed keydata stages nothing; the gossip parse;
the re-open dedup). The receive hook tests (a cleartext Autocrypt header
stages the sender key; a gossip header inside the ciphertext stages the peer
keys; the staged key then verifies a following embedded signature). ONE
gated live harness proof: the send gate emits the Autocrypt header
(send_autocrypt_headers on + a default key), the recipient opens, assert a
collected_keys row (source='autocrypt', peer_email=sender, fingerprint); the
gossip arm (encrypted+signed to ≥2 recipients) stages the gossip keys; the
addr≠From and malformed negatives stage nothing; withdraw copies. No live
model proof; the offline battery is the proof.