Compose drafts now keep an encrypted copy at rest. Astra Mail seals each
draft in its vault (AES-256-GCM under a master key held in your OS
keyring) before storing it, and reopening a draft reads the words back
from that encrypted copy. A draft that cannot be decrypted opens with a
clear "cannot decrypt" notice rather than its contents, and a save that
cannot be sealed is refused rather than stored in the clear. The mail
server's own copy of a draft is unchanged by this release.

Platforms: desktop
Bump: minor
