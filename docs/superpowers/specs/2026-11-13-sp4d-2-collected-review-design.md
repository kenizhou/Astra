# SP4d-2 design: the collected-key review and promote surface

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** Autocrypt/gossip consume stages peer keys silently into collected_keys
(SP4c-3a); receive already offers them to the decrypt/verify resolution, but
there is no user-facing way to review, accept, or reject a staged key.
promote_collected_key exists and is tested in the store but has zero product
callers. This slice wires the review/promote surface: list the staged keys,
accept (promote into crypto_keys + a trust decision), verify (promote + a
verified decision), or reject (remove the staging row + a rejected decision).
The store layer is complete (promote_collected_key with its no-downgrade guard
and provenance-preserving promote_origin, the list/remove queries, the trust and
pin APIs); the gap is everything above it. The CLI key verbs are SP4d-1b (the
desktop-first precedent); the token registry and native security UI are heavier
follow-ups. No engine change; no eval/pipeline bump.

## D1: the app review methods + the promote trust write

A new astra-app child module (the reading_password.rs / keys.rs shape: a
child-module impl App block reading self.crypto_store(); lib.rs at the cap, no
growth) exposes:

- list_collected(account): the account's staged rows (CollectedKeyRow, already
  serde-serialisable with base64 public_data), newest first.
- promote_collected(account, standard, fingerprint, email, verified): calls the
  store promote_collected_key (which drains the staging rows and never
  downgrades a standing keystore row), THEN records the trust decision. This is
  the load-bearing pairing: promote alone is only half an acceptance, the
  explicit decision is what makes the promoted key send-usable (the resolver
  answers Undecided without one) and lifts the S/MIME harvested-anchor
  exclusion. A verified choice always records verified (an upgrade); an
  unverified choice records unverified only when no decision stands (never
  downgrades a standing verified/personal decision). A key with no peer email
  carries no address to decide against; the promote still lands the key.
- reject_collected(account, id, standard, fingerprint, email): removes the
  staging row (remove_collected_key by id) and records a rejected decision, so
  a re-staged copy of the same key is refused by the resolver rather than
  silently re-offered.

A shared store helper records the promote acceptance with the no-downgrade guard
(reusing record_import_trust over put_trust_decision).

## D2: the desktop commands + the web review UI

New desktop commands in commands/crypto.rs (or a child declared from it;
commands.rs is at the cap, no new mod line there) carry the account / standard /
fingerprint / id / verified flag, registered in client/desktop/src/lib.rs. A new
web review component in features/keys/ (joined to the Keys detail) lists the
staged keys with the fingerprint, peer email, source, and standard, and offers
Accept (unverified), Accept as verified, and Reject. The wire types ride
services/crypto.ts (never ipc.ts, at the cap). No secret crosses this surface
(collected keys are public material); the fingerprint is shown so a person can
verify it out of band before choosing Accept as verified.

## D3: the contract

docs/crypto-receive.md gains the review/promote note (the list/accept/verify/
reject flow, the promote-plus-trust pairing, the no-downgrade guard, the rejected
decision) and mirrors it to zh. The capabilities row names the review/promote
landing. A changelog fragment lands.

## Known gaps recorded, not hidden

- The CLI key verbs (keys collected/promote) + the sidecar virtual tools are
  SP4d-1b (the verb lands with its virtual permission tool, needs-approval
  default); the desktop-first precedent.
- The token registry (cryptoki, openpgp-card) and the native security UI are
  heavier follow-ups; token rows and KeyHandle::Token routing already exist.
- A pin affordance (set_pinned_key, marking a promoted key preferred) is a
  follow-up; the promote lands the key send-usable without it.
- The composer recipient-trust UI (showing the trust state at send time) is a
  follow-up.

## Test-first

Every behaviour lands with its red. App tests (list returns the staged rows;
promote lands the key AND records the trust decision so it is send-usable; a
verified promote upgrades, an unverified promote never downgrades a standing
verified decision; reject removes the staging row and records rejected; a
no-email promote still lands the key). ONE gated live harness proof: receive an
Autocrypt message that stages a peer key, list it through the product surface,
promote it (the trust decision lands), and send that peer encrypted mail admitted
by the promote alone; reject a second staged key and assert it is removed;
withdraw copies. No live model proof; the offline battery is the proof.