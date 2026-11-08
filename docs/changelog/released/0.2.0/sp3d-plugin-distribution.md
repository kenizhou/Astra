# Plugin distribution (SP3d)

The .astra-plugin bundle: an uncompressed tar signed ed25519 over the
canonical digest, verified for integrity and publisher identity at install
(tamper, wrong key, and unsigned-in-production each refused). The Plugins
settings page installs behind a consent dialog (every permission starts
unchecked; exactly the checked set is granted), answers the offer first, and
carries the plugins_enabled policy switch; grants and the enabled flag
persist to grants.toml and the boot re-enables the standing set. Production
builds never enable plugins declaring proposed methods. xtask gains
bundle-keygen and bundle-plugin.

Platforms: desktop
Bump: minor
