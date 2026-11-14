S/MIME-encrypted mail now opens readable too: the reading pane decrypts
incoming S/MIME messages with your certificate and verifies the sender's
signature against your trusted anchors, showing whether the chain checked
out, the name matched, or the signature could not be trusted. The real
subject is restored, and a message that cannot be decrypted shows a clear
notice instead of ciphertext. OpenPGP receive shipped alongside it.

Platforms: desktop
Bump: minor