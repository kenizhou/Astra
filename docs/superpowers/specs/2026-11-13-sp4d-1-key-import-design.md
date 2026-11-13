# SP4d-1 design: the key import core

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** The crypto receive/send arc stages and consumes keys, but there is no
user-facing way to import one: the live proofs seed keys programmatically.
docs/crypto-receive.md names "a minimal import path" as the load-bearing
follow-up that makes receive usable beyond tests. The store layer is already
rich (upsert_crypto_key, the trust-decision API, promote_collected_key, the pin
API all exist and are tested); what is missing is everything above it. This
slice wires a desktop key import surface: import your own private/identity key
(enables decrypting received mail) and import a peer's public cert (enables
signature verification and encrypted sends). The CLI key verbs + sidecar virtual
tools are SP4d-1b (the receive arc's desktop-first precedent); the review/promote
surface for collected keys is SP4d-2; the token registry (cryptoki,
openpgp-card) and the native security UI are heavier follow-ups. No engine
change; no eval/pipeline bump.

## D1: the store import functions

Two import paths, deliberately separate to avoid the has_private pollution:

- import_public_cert(account, bytes): parse/validate via astra-crypto-openpgp
  parse_certs (armor-or-binary, at least one cert) + fingerprint_of; store the
  public-only blob (cert_to_public_blob, hex) through upsert_crypto_key with
  key_type:'public', origin:'imported', private_data:None. It MUST NOT route
  through the backend import_key / SqliteKeyStore::put_sync, which hardcode a
  private blob and would set has_private true, polluting the receive
  decryption-candidate filter (receive.rs:188) and the resolver's Personal/Ready
  branch (resolve.rs:170). S/MIME parity uses the existing cert parse
  (fingerprint_of_cert_der / cert_summary_of_der) with key_type:'cert'. Returns
  the CryptoKeyRow.
- The private/identity key import reuses the EXISTING backend import_key
  (passphrase-gated secret decrypt, persist_cert) which correctly records a
  private blob; this slice wires it, it does not reinvent it.

A shared trust-write helper (put_trust_decision verified/unverified) is exposed
for the import surface and reused by SP4d-2's promote (promote alone is only
half an acceptance: the explicit trust decision is what makes a key send-usable
and lifts the S/MIME harvested-anchor exclusion).

## D2: the app method + desktop command + web affordance

A new astra-app child module (the reading_password.rs shape: a child-module
impl App block reading self.crypto_store(); lib.rs is at the cap, no growth)
exposes import_key(account, bytes, optional passphrase) dispatching to the
public-cert or private path by whether the input carries secret material, and
list_keys(account). A new desktop command in commands/crypto.rs (64 lines, room;
commands.rs is at the cap so no new mod line there) carries the bytes/passphrase
in-process (the submit_reading_password precedent: secrets stay in-process,
never logged), registered in client/desktop/src/lib.rs. A minimal web import
affordance (file/paste) in a new features/keys/ component, wire types in
services/crypto.ts (never ipc.ts, at the cap), joined to the settings rail. The
passphrase is held only in memory, zeroized, never logged or persisted.

## D3: the contract

docs/crypto-receive.md gains the key-import note (the two paths, the has_private
guard, the in-memory passphrase, the trust write) and mirrors it to zh. The
capabilities row names the key-import landing. A changelog fragment lands.

## Known gaps recorded, not hidden

- The CLI key verbs (keys import/list) + the sidecar virtual tools are SP4d-1b
  (the verb lands with its virtual permission tool, needs-approval default); the
  receive arc's desktop-first precedent. The CLI messages read still reads the
  engine body undecrypted; a CLI security field is a separately named follow-up.
- The review/promote surface for collected keys is SP4d-2 (promote + trust write
  + reject/delete + review UI); staging stays silent until then.
- The token registry (cryptoki, openpgp-card) and the native security UI are
  heavier follow-ups; token rows and KeyHandle::Token routing already exist.
- Key generation, export, and the composer recipient-trust UI are follow-ups.

## Test-first

Every behaviour lands with its red. Store tests (import_public_cert round-trips
a public cert with has_private false; a malformed input stages nothing and never
panics; the fingerprint/user-id are extracted; the public path never sets a
private blob; the trust-write helper records a decision). App/command tests
(import dispatches public vs private; list returns the rows; the passphrase
never leaks). ONE gated live harness proof: import a peer public cert and own
private key through the product surface, then receive an encrypted+signed
message decrypts and verifies (no programmatic seeding); withdraw copies. No
live model proof; the offline battery is the proof.