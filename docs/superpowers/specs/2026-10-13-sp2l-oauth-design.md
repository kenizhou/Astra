# SP2l OAuth Foundation Design

Date: 2026-10-13
Status: proposed
Parent spec: `docs/superpowers/specs/2026-08-...-workflow-design` (sections 5, 15)
Port contracts: mail-calendar `docs/provider-oauth.md` in substance (the
rules section carried whole) and the `mailcal-oauth` crate as the code
reference; `mail-oauth.md` and `jmap.md` defer their halves to SP2v/SP2o.

## 1. Purpose

The OAuth foundation the provider ladder stands on: the pure
Authorization-Code-with-PKCE client (begin, complete, refresh), the
grant-refusal classification, the dynamic discovery and registration for
servers Astra has no client registered with, the system-browser sign-in
session with the deep-link redirect, and the credential transaction whose
"the core owns when, the host owns where" split the rules state. Nothing
here signs into a mail account yet: the Graph and Google slices (SP2m,
SP2n) wire the providers over this floor; what ships now is the floor.

## 2. Decisions

- **D1 (the crate)**: `astra-oauth`, ported from `mailcal-oauth` with
  fidelity to its contracts and tests: `Pkce` (S256) + `random_state`;
  `exchange_code`/`refresh`/`TokenSet`; `OAuthProviderConfig`
  (`microsoft()`/`google()`), `AuthStyle`, the scope lists;
  `GrantedScopes` and `GrantRefusal` (Dead / Underscoped / Indeterminate,
  classified never text-matched beyond the two refusal codes);
  `discover_protected_resource` / `discover_auth_server` / `register_client`
  (RFC 9728 -> 8414 -> 7591); `credentials.rs` as the only holder of the
  pre-registered client ids (a build given none still connects mail: the
  routes that need a registration are not offered); `reach` (the token
  request's transport classification); the `Secret` whose Debug redacts.
  The refresh names NO scope (the not-negotiable rule); the granted-set
  cache follows the token response's word.
- **D2 (the browser session)**: the system browser over a Tauri shell
  open, the redirect captured by `tauri-plugin-deep-link` on the custom
  scheme; NEVER an embedded WebView. The client passes the raw callback
  URL back to the core; it never parses tokens itself. The `state` CSRF
  check stands in `complete`.
- **D3 (the credential transaction)**: tokens live only in the OS
  keystore (the refresh token the single at-rest secret; access tokens
  minted on demand, never persisted); the secret redacts itself. One
  `AccountCredentialStore` port (`persist`, `delete`) serves every family;
  both report success and failure, and the core answers per the rules:
  an add rolls back on failure, a rotation logs what the next launch
  will face, a removal returns the error rather than reverting.
- **D4 (the wizard surface)**: the account setup gains the OAuth path's
  skeleton: where a provider's sign-in exists the setup offers it beside
  the password form (the detection's recommendation already routes);
  the refusal states surface (Dead and Underscoped both ask for consent
  again; Indeterminate changes nothing about the sign-in's standing).
  The Microsoft and Google routes render only when `credentials` carries
  their registrations.
- **D5 (the replay-safety rules)**: the four rules (single-flight
  refresh, never-store-a-superseded-token, the rotation write through
  the port, the dead-grant stop) port with their tests; a server that
  treats a replayed refresh token as theft never receives one.

## 3. Testing

- Unit (astra-oauth, pure, no network): the ported test batteries
  (pkce's S256 vectors, the token endpoint's request/response shapes,
  the refusal classification incl. the never-text-match boundary, the
  scope rules incl. the no-scope-on-refresh pin, discovery's RFC
  documents, registration's client minting, the Secret's redaction).
- The host session (desktop tests): the deep-link scheme capture, the
  raw-URL hand-back, the state mismatch refused.
- The credential transaction: the three answer paths (add rollback,
  rotation log, removal error) with a scripted store.
- The wizard surface (vitest): the OAuth offer beside the password form
  when a registration exists; absent when it does not; the refusal
  sentences.
- Live (gated, harness): the discovery half against Stalwart's JMAP
  `/.well-known` (a real RFC 9728 document); no live token exchange
  exists in the harness (recorded: the real sign-ins are SP2m/SP2n with
  the owner's accounts).

## 4. Known gaps (pre-recorded)

- No mail account type consumes the floor yet: Graph (SP2m) and Google
  (SP2n) wire the providers; JMAP OAuth (SP2o) and IMAP XOAUTH2 (SP2v)
  defer to their slices.
- The reconnect banner and the expired-sign-in row surface with the
  first OAuth account type that can hit them (SP2m); the classification
  they read ships here.
- The registration document's abuse of RFC 7591 against public test
  servers is unproven live (no harness server implements it; recorded).
- Windows' browser choice is the OS default; no per-browser preference.
