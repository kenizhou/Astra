# SP2v plan: IMAP/SMTP OAuth sign-in

Spec: docs/superpowers/specs/2026-10-25-sp2v-mail-oauth-design.md. Branch
`sp2v-mail-oauth` from main at the docs commit. Each task: reds first,
the implementation, the battery, a conventional commit.

### Task 1: the core (D2's chain, D3's grant and source)

**Files:** crates/astra-account/src (the mail OAuth grant shape beside
the imap section; the token-bearing CredentialSource over the shared
GraphTokenSource; the issuer chain: the autoconfig route (the
autodetect crate's parsed field crossing), the two well-known
candidates, the fail-soft; the registration cache; the probe wrapper
over the engine's verb); the config loader/saver arms; fakes.

- [ ] Tests first; commit `feat: feed the mail oauth core`.

### Task 2: the host and surfaces (D1's screens, D3's dial, D4)

**Files:** client/desktop/src (the boot's imap grant arm with the dial
table and the rotation sink; the completion; the probe command the
wizard's ask rides); client/web/src (the IMAP step's three screens over
the probe answer; the sign-in button and the demoted password; the
expired row's imap_smtp shape); the wire.

- [ ] Tests first; commit `feat: surface the mail oauth sign-in`.

### Task 3: the proofs + the docs

**Files:** the gated live proofs (the harness's real capability line
pinned on both legs; the discovery's fail-soft over the harness);
docs/mail-oauth.md + zh (created: the eleven rules ported, scoped to
what ships); docs/provider-oauth.md's deferred sentences resolved; the
capabilities row 26 final; the fragment.

- [ ] Commit `feat: prove the mail oauth leg and record it`.

### Task 4: close-out

- [ ] Full battery; branch `sp2v-mail-oauth` pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: the chain + grant + source (T1), the dial + screens
  (T2), proofs + docs (T3).
- Interface chain: the core shapes (T1) -> the host and screens (T2)
  -> live + docs (T3).
- Risks: the probe must gate on the DIALLED server (the harness's TLS
  acceptance walk, not a second dial); the token source must not fight
  the engine's one-renewal (the engine asks, the source refreshes at
  most once per refusal); the issuer candidates must respect the
  registrable-domain rule (no public-suffix leakage); the static table
  ships empty and no code path may invent an id; the password demotion
  must never remove the field (the reference rule).
