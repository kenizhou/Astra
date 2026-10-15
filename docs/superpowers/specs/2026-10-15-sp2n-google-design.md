# SP2n Google Native APIs Design

Date: 2026-10-15
Status: proposed
Parent spec: `docs/superpowers/specs/2026-08-...-workflow-design` (sections 5, 15)
Port contracts: mail-calendar `docs/provider-oauth.md` (the Google section:
the Desktop client's loopback redirect, the non-confidential secret, the
scope table, `access_type=offline` + `prompt=consent`, no calendar
re-auth) and MC's `crates/mailcal-account/src/google/` as the code
reference.

## 1. Purpose

The second provider over the OAuth floor: a Google account signs in
through the browser flow (the Desktop client's loopback redirect captured
by a one-shot local listener), its mail and calendar dial through the
engine's `provider-google` adapter behind a token source shaped like
Graph's, and the scope set follows what the account is used for.

## 2. Decisions

- **D1 (the loopback redirect)**: the Desktop client cannot use custom
  schemes; the redirect is `http://127.0.0.1:<ephemeral-port>/` and the
  port is baked into the redirect, so the listener binds BEFORE the
  authorization URL mints. A one-shot bounded `TcpListener` (the SP2l
  session grows the Google arm: bind the port, mint the URL with the
  loopback redirect, open the browser, capture the one request, answer it
  with Google's own minimal HTML, hand the raw URL to the core's
  parser). Microsoft keeps the custom scheme; the session routes by
  provider.
- **D2 (the non-confidential secret)**: Google's token endpoint rejects
  the Desktop PKCE exchange without the client secret; it rides the
  exchange AND the refresh (persisted in the `[google]` document so a
  launch-later refresh still has it). It is not confidential (Google's
  own guidance), PKCE stays the protection, and it never logs.
- **D3 (the scope set)**: `GOOGLE_SCOPES` (already ported in
  SP2l's provider.rs) with `access_type=offline` + `prompt=consent` (the
  refresh token only comes back with both); the use-for table scoped to
  what ships (mail + calendar first; contacts and the directory ride
  their slices; `userinfo.email` always, because it names the account);
  a granted set missing a use's scope does not open that use; no
  calendar re-auth step for Google (the banner stays Microsoft-only).
- **D4 (the account dial)**: the engine's `GoogleClient` behind a
  refreshing wrapper shaped like Graph's (the delegate cache, the
  mint-before-call, the refusal classification reuse); the mail provider
  and the calendar provider; the `[google]` config section additive.
  Google's refresh tokens do not rotate on refresh (the rotation write
  is the backstop that rarely fires, but it is not optional).
- **D5 (the account's address)**: `userinfo.email` names the account
  (the account's address from the token's own profile read); the derived
  id `email@google`.

## 3. Testing

- Unit (astra-account over the scripted token endpoint): the Google
  token source's refresh (the secret on the refresh; the no-rotation
  backstop); the scope table's use-for mapping; the wrapper's delegate
  discipline (reuse Graph's test shapes).
- The loopback capture (the desktop suite): the listener binds before
  the URL mints (the port is IN the redirect); the one-shot capture; the
  minimal HTML answer; the raw URL handed whole.
- The completion (the SP2m lifecycle pattern): the sign-in, the address
  read, the add/rotate, the boot re-dial, the dead grant's row.
- Live (gated, the owner's Google Cloud registration + the test
  account): the sign-in over the real browser, the mail sync, a calendar
  read/write. As with Graph: the harness cannot serve Google; the live
  proof awaits the registration and its absence is a recorded gap.

## 4. Known gaps (pre-recorded)

- The live proof depends on the registration (the Early Access test-user
  allow-list is Google's own gate while the app is unverified; MC's
  notice-and-checkbox surface ports with it when it becomes reachable).
- Contacts and the Workspace directory ride their slices; the sender-name
  write (`sendAs.patch`) rides the accounts-page slice.
- No calendar re-auth for Google (the refused calendar stays closed);
  the banner stays Microsoft-only.
- The full-mail scope grants permanent delete; no Google tool or surface
  offers one (the MCP slice already cut it).
