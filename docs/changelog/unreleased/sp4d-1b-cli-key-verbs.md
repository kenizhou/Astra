The assistant can now manage encryption keys through the agent surface.
Five new CLI verbs (keys list, keys collected, keys import, keys promote,
keys reject) read and review the account's key store, and the sidecar
exposes them as five virtual permission tools: the two reads run freely,
while importing a public cert and accepting or rejecting a collected key
suspend behind an approval card you decide. Accepting records the trust
decision that makes the key send-usable, and a rejection stands against
later acceptance without verification. Only public key material crosses
this surface: importing a private key stays in the desktop Settings Keys
page, where the vault master key lives, and the CLI refuses secret-bearing
input with a pointer there.

Platforms: desktop
Bump: minor
