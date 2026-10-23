# Built-in S/MIME and OpenPGP sending (SP4b)

Platforms: desktop
Bump: minor

- The built-in crypto's send half ships (the 2026-10-20 decision: encryption
  is a product feature, not a plugin). Each account gains an Encryption
  (built-in) setting on the Accounts page (`None` / `OpenPGP` / `S/MIME`).
  With one set, sends assemble in Astra, gate through the ported kylins
  `apply_crypto` (trust-gated recipients with no plaintext fallback,
  protected subject headers, encrypt-to-self, sign-then-encrypt
  composition, OpenPGP Autocrypt-Gossip), and travel the new Astra-side
  durable crypto outbox instead of the engine's queue, drained by a small
  explicit SMTP submitter with the Sent copy filed over IMAP. A mid-DATA
  loss surfaces as an unconfirmed row in the outbox panel; the user's
  answer settles or re-sends it.
- Key material, trust decisions, staged keys, pins, and the outbox live in
  a new `crypto.sqlite` beside the AI store; soft private keys wrap at rest
  under an AAD-bound AES-256-GCM master key in the OS keyring.
- The receive half (decrypt/verify in the reading pane, encrypted drafts at
  rest, the security banner) lands in SP4c; hardware tokens and the native
  security UI in SP4d.
