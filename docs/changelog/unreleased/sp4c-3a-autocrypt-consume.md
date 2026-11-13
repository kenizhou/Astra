Opening an encrypted OpenPGP message now collects the Autocrypt keys it
carries: the sender's key from a cleartext Autocrypt header (only when its
address matches the From address) and the recipient peer keys from the
Autocrypt-Gossip headers inside the ciphertext. The keys stage silently in
the collected area, so a following message from a known peer verifies its
embedded signature instead of reporting an unknown signer. Staging never
changes the message's own decrypt verdict, and a key already collected is
not duplicated. The review and acceptance surface for collected keys
follows separately.

Platforms: desktop
Bump: minor