# SP4c-3c design: the W9 password receive flow

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** The third leg of SP4c-3: password-protected OpenPGP messages
(SKESK). The crypto ops exist both directions (decrypt_symmetric, has_skesk,
the typed WrongPassphrase) but are unwired for receive: receive_openpgp only
does PKESK key-based decrypt, so a SKESK-only message fails closed (NoKey /
Failed) and never prompts. This slice wires the receive password flow: detect
the SKESK, surface a needs-password verdict, and decrypt with a user-supplied
passphrase. The passphrase is held only in memory, never logged or persisted
(the password.rs precedent). No engine change; no eval / pipeline bump.

## D1: the needs-password verdict + the password orchestrator

A new `ReceiveState::NeedsPassword` arm (additive) and a password orchestrator
entry `receive_openpgp_password(store, account, message, source, passphrase)`:

- After unwrap, `has_skesk(&ciphertext)` detects a passphrase-protected
  message. When SKESK is present and no PKESK key fits, the verdict is
  NeedsPassword (not NoKey/Failed). The fail-closed arms are preserved for the
  other cases.
- The password entry calls `decrypt_symmetric` with the same known_signers /
  extra_signer_blobs resolution as the key path, so an embedded signature
  still verifies (an unknown signer degrades, never aborts). On success it
  writes the ok result row + restores the protected subject + returns the
  legs. On WrongPassphrase it returns a typed re-prompt signal (never a
  plaintext leak).
- The needs-password state is a transient interaction, not a persisted row
  (the decrypt_state CHECK stays ok/no-key/failed/n/a); success writes ok.

## D2: the decoder arm + the password prompt

The CryptoDecoder maps NeedsPassword to a "password required" banner + a
prompt affordance. A new password dialog (its own component, the
useDialogFocus idiom) collects the passphrase; a new desktop command carries
it in-process (web→desktop, the crypto_wrapper precedent: secrets stay
in-process, never logged/persisted) and re-runs the password orchestrator over
a fresh engine.message_source, then refreshes the reading snapshot. On
WrongPassphrase the dialog re-prompts; on cancel the verdict stays
needs-password (fail closed). The new wire type lands in the owning feature
service, never ipc.ts (at the cap). The reading re-entry splits into a new
module (reading.rs is near the cap).

## D3: the contract

docs/crypto-receive.md gains the W9 password-flow note (the SKESK detection,
the needs-password verdict, the in-memory passphrase, the re-prompt) and
mirrors it to zh. The capabilities OpenPGP row's note names the password
receive landing. A changelog fragment lands.

## Known gaps recorded, not hidden

- The W9 SEND side is itself unwired (apply_password_crypto has no callers,
  SendDraft has no password_protected field); the live proof hand-builds a
  SKESK message via encrypt_symmetric. Wiring W9 send is a coordination
  follow-up.
- The passphrase is never persisted; a re-open re-prompts (no cached
  passphrase). A session passphrase cache is a follow-up.
- The password dialog is shell-only (the reading pane), like the other
  receive surfaces.

## Test-first

Every behaviour lands with its red. The orchestrator tests (a SKESK message →
NeedsPassword; the correct passphrase → ok + the words + the restored subject;
WrongPassphrase → the re-prompt signal, no plaintext; an embedded signature
verifies / an unknown signer degrades). The decoder tests (the needs-password
banner). The command tests (the passphrase re-entry decrypts; cancel stays
needs-password). ONE gated live harness proof: hand-build a SKESK message via
encrypt_symmetric, deliver it, open → needs-password; submit the passphrase →
the words + the restored subject; a wrong passphrase → re-prompt; withdraw
copies. No live model proof; the offline battery is the proof.