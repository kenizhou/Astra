# SP2m Microsoft Graph Design

Date: 2026-10-14
Status: proposed
Parent spec: `docs/superpowers/specs/2026-08-...-workflow-design` (sections 5, 15)
Port contracts: mail-calendar `docs/provider-oauth.md` (the Microsoft half:
the rules already landed with SP2l; this slice ports the account wiring) and
MC's `crates/mailcal-account/src/graph/` as the code reference.

## 1. Purpose

The first provider over the OAuth floor: a Microsoft account signs in
through the browser flow, its mail and calendars dial through the engine's
`provider-graph` adapter, the token refreshes on demand through a single
token source whose rotations land in the OS store, and the surfaces a
Microsoft account owes (the reconnect banner when a permission went
missing, the expired-sign-in row) arrive with it.

## 2. Decisions

- **D1 (the token source)**: `GraphTokenSource` ported from MC's
  `graph/token_source.rs` (468 lines): one per account, seeded by the
  sign-in, minting access tokens before expiry under the skew rule,
  refreshing through `astra-oauth`'s `refresh`, classifying refusals by
  the ported `GrantRefusal` (Dead/Underscoped stop retrying and surface;
  Indeterminate changes nothing), and rotating the stored refresh token
  through the SP2l `CredentialTransaction`'s `rotate` path (the SP2m
  anchor the foundation recorded). One refresher per credential: the
  source registers with the account entry before the dial so a rotation
  during the dial lands somewhere.
- **D2 (the account dial)**: the engine's `GraphClient::for_mailbox` over
  the token source's minted token, `GraphProvider` for the mail folder,
  `GraphCalendarProvider` for the default calendar (the principal `Me`;
  shared mailboxes defer), each behind the engine's `Provider` trait and
  assembled into the host account beside the mail providers the SP2h
  pattern established (the capability-scoped selection already picks
  calendars).
- **D3 (the sign-in completing into an account)**: the SP2l wizard's
  success sentence becomes the real add: the token exchange's address
  lookup names the account; the grant persists through the transaction's
  `add` (the rollback on refusal); the registry entry, the dial, the
  first sync; a re-consent (signing in again for an account that stands)
  keeps the stored shape and goes through `rotate`, never `add`.
- **D4 (the refusal surfaces)**: the reconnect banner (a Graph account
  missing a needed permission, e.g. consent revoked server-side; one tap
  re-grants the full scope set by re-running the sign-in) and the
  expired-sign-in row (a grant gone: reported as such with the offer to
  sign in again, never badged as a server outage). Both read the token
  source's classification; both surface on the accounts page and the
  folder pane's per-account badge. The channel is decided: a standing
  refusal state is **snapshot data**, an additive `grant` field on the
  settings snapshot's account rows, sourced through the host account-info
  seam from the boot dial and the sign-in's completion (the classification
  is born host-side); the SP2l completion event stays the wizard's one-shot
  channel, because a surface that mounts late or a relaunch must pull
  standing state, never catch an emission.
- **D5 (the scope set)**: `MICROSOFT_GRAPH_SCOPES` (already ported in
  SP2l's provider.rs) asked at authorisation for what the account is used
  for (mail + calendar first; contacts and the directory ride their
  slices); `offline_access` for the refresh token; the granted-set cache
  answering at launch and offline.

## 3. Testing

- Unit (astra-account over a scripted token endpoint): the token source's
  refresh cycle (the skew, the single-flight, the rotation write through
  the transaction, the refusal classification's three answers); the dial
  (the seeded token; the re-dial on expiry).
- The sign-in completing into an account (the desktop suite over the
  session's fed-callback seam): the add path, the rollback on a refused
  store, the re-consent through rotate.
- The refusal surfaces (the web): the banner, the row, the one-tap
  re-grant.
- Live (gated, the owner's outlook.com account + the Azure registration):
  the sign-in over the real browser, the mail sync, a calendar read, a
  write, the refresh rotation. The harness cannot serve Graph; the live
  proof needs the registration and the account, and its absence is a
  recorded gap, never a faked pass.

## 4. Known gaps (pre-recorded)

- The live proof depends on the Azure app registration (the owner's);
  without it the slice ships proven offline and the live proof lands when
  the registration does.
- Shared/other mailboxes (`MailboxPrincipal::user`) defer: one account is
  its own mailbox today.
- Contacts and the colleague directory over Graph defer to their slices
  (the contacts surface already rides the people index; the directory
  needs the `colleagues` choice).
- The sender-name write (`sendAs.patch`) is Graph's own; it rides the
  accounts page slice that owns the field.
- The registration's client secret: Microsoft's public client carries
  none; if the registration turns out to require one, the divergence from
  SP2l's credentials shape is escalated in the PR.
- The banner's re-grant targets by `login_hint` alone: completing the
  browser flow as a different address adds a second account rather than
  being refused (MC's complete_account_consent pins the target and
  refuses the mismatch). The wizard-begun flow has the same shape; the
  pin rides the provider slices' completion rework.
- The dial is a free function where mail-calendar's is a registry-gated
  `AccountDial` (constructible only through the registry, which is what
  made its pre-register ordering unskippable): Astra's dial table stands
  in, and the ordering is carried by the completion's own sequence and
  its tests. The per-use capability gating MC derives from the stored
  grant defers with it; the 403 probe reaches the same re-consent
  outcome.
- A grant that dies mid-session is unobserved until a dial runs again:
  the sync pass swallows per-account failures, so the expired row appears
  at the next launch, not the moment the provider revokes (MC rule 12's
  sync-side classifier is its own change).
- An offline relaunch classifies nothing: only a dial that reaches the
  token endpoint can answer Dead, so the accounts page holds no expired
  row until the network returns (MC rule 12 binds "not suppressed while
  offline"; the last classification is not persisted).
