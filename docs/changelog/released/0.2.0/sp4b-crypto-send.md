# Built-in S/MIME and OpenPGP sending (SP4b, through the engine seam)

Platforms: desktop
Bump: minor

- The built-in crypto's send half ships (the 2026-10-20 decision: encryption
  is a product feature, not a plugin). Each account gains an Encryption
  (built-in) setting on the Accounts page (`None` / `OpenPGP` / `S/MIME`);
  the write re-dials the account, so the method takes effect with the next
  send, and an account without IMAP/SMTP submission settings refuses the
  write rather than silently sending plaintext.
- With a method set, sends ride the **engine's own outbox** through its
  MIME-wrap seam: the drain assembles the message and the account's
  installed wrapper gates it through the ported kylins `apply_crypto`
  (trust-gated recipients with no plaintext fallback, protected subject
  headers, encrypt-to-self, sign-then-encrypt composition, OpenPGP
  Autocrypt-Gossip), the wire variant the same ciphertext minus the `Bcc`
  header, the Sent copy filed by the engine. A recipient the gate cannot
  encrypt to fails the send's row with the reason; Send Now retries once
  the keys are fixed. (The SP4b draft of this shipped an Astra-side crypto
  outbox instead; it was retired before any release, and the
  `crypto_outbox` table is dropped.)
- Key material, trust decisions, staged keys, and pins live in a new
  `crypto.sqlite` beside the AI store; soft private keys wrap at rest
  under an AAD-bound AES-256-GCM master key in the OS keyring.
- The receive half (decrypt/verify in the reading pane, encrypted drafts at
  rest, the security banner) lands in SP4c; hardware tokens and the native
  security UI in SP4d.
