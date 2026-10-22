# The crypto workspace, vendored (SP4a)

The kylins crypto workspace lands built-in as crates/astra-crypto-core,
astra-crypto-openpgp, and astra-crypto-smime (decided built-in, not a
plugin): the engine API, Sequoia OpenPGP backend, and S/MIME backend with
chain validation, CMS build/parse, PKCS#12 import, and hardware-token
routing, all with the upstream test battery green in-tree (232 tests,
including gpg/sq and openssl interop where the CLIs exist). The cms builder
is the kylins vendored snapshot: the published 0.3.0-rc.0 was evaluated and
rejected with the cause named (its kari/ECC path misses the snapshot's KEK
fix, and pkcs12 pins pre.1 exactly). The 500-line gate gains an
upstream-ported exemption so the ported files stay diffable with their
source. The Astra-side store, send gate, read pipeline, and security UI land
in SP4b/SP4c.

Platforms: desktop
Bump: minor
