# SP2w plan: the account completion batch

Spec: docs/superpowers/specs/2026-11-07-sp2w-account-completion-design.md. Branch
`sp2w-account-completion` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit.

### Task 1: the core (D1's identity half, D4's resolution)

**Files:** crates/astra-app/src (the sender-name service over the engine's
identity verbs: adopt-at-join, the writable push, the local preference for
IMAP; the settings snapshot's identity fields; the name ask state); the
vendor family's three rows resolved in docs/capabilities.md + zh and the
backlog's decision section; fakes' identity legs.

- [ ] Tests first; commit `feat: feed the account completion core`.

### Task 2: the host and surfaces (D1's ask, D2's re-grant, D3's switcher)

**Files:** client/desktop/src (the JMAP re-grant commands over the
account-layer arms; the identity reads); client/web/src (the wizard's
ask-once name step; the settings card's three controls; the From picker
and account rows carrying the name; the expired rows' re-grant actions;
the folder pane's account-header switcher and the list header's
whose-mail label).

- [ ] Tests first; commit `feat: surface the account completion`.

### Task 3: the proofs + the docs

**Files:** the gated live proofs (a JMAP identity adopt and name write
over the harness; the re-grant's grant swap over the scripted harness
shape); docs/sending.md extended (the sender's name contract) + zh;
docs/jmap.md + provider-oauth.md + zh resolutions; the capabilities
rows (the name, the JMAP re-grant, the switcher final; the vendor three
resolved); the fragment.

- [ ] Commit `feat: prove the account completion live and record it`.

### Task 4: close-out

- [ ] Full battery; branch `sp2w-account-completion` pushed; PR (squash expected); ledger cleanup.

## Self-review notes

- Spec coverage: D1's core half + D4 (T1), the ask + re-grant + switcher
  surfaces (T2), live + docs (T3).
- Risks: the identity push is per-family (Google writable, JMAP writable
  through the refreshing wrapper, Graph read-only, IMAP local-only) —
  pin each; the re-grant rides the credential transaction (the
  register-before-dial ordering); the switcher must not fight the
  folder selection (two scopes, one verb family).
