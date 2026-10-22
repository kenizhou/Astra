# SP4b plan: the crypto store and the send gate

Spec: docs/superpowers/specs/2026-10-22-sp4b-crypto-send-design.md. Branch
`sp4b-crypto-integration` from main. Each task: reds first, the
implementation, the battery, a conventional commit.

### Task 1: the crypto store (D1)

**Files:** crates/astra-crypto-store/ (the schema verbatim, the
SqliteKeyStore port, trust decisions + recipient resolution, default signing
key, recipient_trust_failures) with a MemoryKeyStore-shaped test fixture
mirroring kylins'.

- [ ] Tests first; commit `feat: open the crypto store`.

### Task 2: the send gate (D2)

**Files:** crates/astra-crypto-core/src/apply.rs (the apply_crypto port),
the pgp_mime and protected-headers/autocrypt helpers it needs, the
CryptoSendError enum; kylins' apply fixtures adapted to the Astra store.

- [ ] Tests first; commit `feat: gate sends with apply_crypto verbatim`.

### Task 3: the outbox and the transport (D3, D4)

**Files:** crates/astra-crypto-store/src/{outbox.rs,transport.rs} (the
queue table and its states, the SMTP submit + Sent filing, engine-tls);
crates/astra-app (the crypto branch in the submit arms, the crypto-method
preference, the Sending surface's crypto rows, the composer sign/encrypt
flags on the submit intents); wire pins.

- [ ] Tests first; commit `feat: send crypto mail through the Astra outbox`.

### Task 4: the harness smoke and the record (D5)

**Files:** crates/astra-app/src/tests_live_harness/crypto_send.rs (generate,
encrypted send astra1 -> astra2, Sent copy filed, missing-key recipient
blocked); docs/plugins.md's Track B update + zh; capabilities rows + zh;
the changelog fragment.

- [ ] Commit `feat: prove the crypto send live and record it`; close out
  (full battery, push, PR).

## Self-review notes

- Spec coverage: store (T1), gate (T2), outbox + transport + switch (T3),
  smoke + record (T4). Every D lands in exactly one task.
- Interface chain: the store (T1) -> the gate consumes it (T2) -> the seam
  drives it (T3) -> live proof (T4).
- Risks: assembly must use the engine's own assembler or the wire differs
  from non-crypto sends; the gate must run before any engine queue write
  (never after); the crypto queue's rows must never double-submit (the
  unconfirmed answer is the only re-send path); the bypass's transport must
  fail typed, never silently, on any provider it cannot serve.
