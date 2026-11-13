A passphrase-protected OpenPGP message now prompts for its passphrase
instead of failing closed. When an encrypted message carries a SKESK slot
and no key fits, the reading pane shows a "passphrase required" banner and
opens a password dialog. Enter the passphrase to decrypt the message: the
words and the restored protected subject render exactly as a key-decrypted
message. A wrong passphrase re-prompts, naming the refusal; cancel leaves
the message closed. The passphrase stays in memory only: it is never
logged or persisted, and a re-open prompts again.

Platforms: desktop
Bump: minor