The virtual permission tools mapping is now an explicit, test-pinned
contract: the gating map in cli.md names every tool's CLI verb, policy
level, and redacted preview shape, held by pins on the host and the
sidecar. Five approval previews (search, mark read, set flagged,
archive, move to trash) became shaped: the permission audit row now
carries ids and counts for them instead of the generic field count.

Platforms: desktop
Bump: none