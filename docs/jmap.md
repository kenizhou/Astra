# JMAP accounts: the Astra Mail contract

**Scope.** What a JMAP account is in Astra Mail: the one secret it stores,
the base URL its setup derives, the identity and the one provider its whole
account syncs through, the browser sign-in a server that advertises
discoverable OAuth connects it by, and the surfaces that show it. Binding on
every surface that ships a JMAP account. Ported in substance from
mail-calendar's `agent-guidance/jmap.md` and `providers.md`, scoped to what
Astra Mail ships today; the gaps are recorded at the end, never left silent.

**Principle.** *A JMAP account is a base URL, one secret, and whatever the
server's own session says it has.* Nothing about a JMAP account is guessed:
the capabilities (mail, submission, calendars) are read from the session
resource the dial fetched, the authentication scheme is whichever the
server's own `401` challenge names, and the account-wide scope means one
connection serves every folder. Astra adds no per-provider knowledge to this
family; the standards are the provider.

## The one secret

A JMAP account stores exactly one credential, as `password`, whatever the
person pasted: a login password, an app-specific password, or an API token.
Three rules hold it to one:

- **The username-bearing form is strictly more capable.** The secret is
  stored with the account's email address as the username, and the engine's
  transport presents it as HTTP Basic or re-frames it as Bearer when the
  server challenges for that, so one stored form serves a server that changes
  its mind about the wire format. A bare token has no username to build a
  Basic header from.
- **A legacy `token` key folds into `password` at load.** A document written
  before the two fields were collapsed to one still connects; the fold keeps
  the `token` value, the credential its account was last connected with. The
  key is read and folded, never written again, and no code path can ever see
  both secrets at once.
- **A browser-signed-in account stores a grant instead, never both.** The
  `[jmap.oauth]` block's refresh token is the account's one credential, and a
  document somehow carrying both a password and a grant resolves to the
  grant: the account was connected by signing in. The host never writes both.

The one-secret rule is load-bearing hardening, not tidiness: a client that
stores a password and a token for one account has two credentials that can
disagree about what they may do, and the server's revocation of either one
reads as the account failing for a reason the stored shape cannot name. One
secret, one refusal, one remedy.

## The base URL

The setup form's server field resolves through one derivation every stored
JMAP document and every connect walks, so a discovered account and a
hand-entered one cannot disagree about which server they mean:

- **Blank** derives `https://<email-domain>`, relying on RFC 8620 autodiscovery
  at `/.well-known/jmap`.
- **A bare host** is prefixed `https://`.
- **An explicit `http://` survives only for a loopback host** (a literal
  `localhost`, or a loopback IP in any notation). On any other host it is
  upgraded to `https://`, because a real account always dials TLS and a
  person who typed `http://` at a public server meant the server, not the
  cleartext. This agrees with the discovery rule the autodetect crate holds
  for the same origin class.

## The identity and the dial

A JMAP account's id is `address@jmap:<host>` (the resolved host,
lowercased), so it never collides with an IMAP account for the same address
on the same host, nor with the OAuth families' `address@graph` and
`address@google`. The tag cannot appear in a real IMAP server name, so the
id spaces stay disjoint.

The dial connects **one provider for the whole account**. JMAP's mail scope
is account-wide and each message carries its `mailboxIds` membership, so a
single connection syncs every folder and there are no per-folder providers
to bind; a folder opened before the first pass reconnects this same
account-wide provider rather than minting a second one. The capabilities the
dial snapshots from the session decide what the account has: mail, submission,
and calendars are read from the server's own session resource, never guessed
from the config, and the calendar read rides exactly that advertisement
(a session that serves no calendars capability serves the account no
calendar surface).

The proof of the whole leg is live and gated: the password account connects
over a real JMAP server, one refresh lists the account's folders through the
one provider, the seeded notes render newest first, a rich note opens through
the product's own reading pipeline, a send round-trips to a second JMAP
account with the sender's copy filed in Sent, and the calendar the session
advertised reads into the calendar surface. The suite withdraws everything
it sends, so the server keeps its shape for the suites that pin it.

## The OAuth sign-in

A JMAP server that advertises discoverable OAuth (RFC 9728) offers a second
way in, with no per-provider code anywhere: the chain is the standards, the
client registers itself, and Astra names no vendor.

- **The discovery chain is RFC 9728 to RFC 8414 to RFC 7591.** The
  protected-resource document at the session URL names the authorization
  server; the authorization server's metadata names the endpoints and, if it
  publishes one, the registration endpoint; dynamic registration mints the
  client. A server that advertises no registration endpoint cannot be offered
  sign-in, because there is no way to become a client.
- **Fail soft, always.** Every miss along the chain leaves the password field
  standing, never an error: the wizard offers the sign-in only where the
  server's own advertisement supports it, and a person whose server offers
  none sees the ordinary password form. The offer probe is a read of the
  server's own documents, nothing more.
- **Registration is session-cached.** Dynamic registration mints a new client
  id every call, so without a cache every cancelled or failed attempt would
  orphan another registration on the person's account. The cache keys on
  `(issuer, redirect_uri)`, and once a sign-in completes the client id
  persists with the account and no further registration happens.
- **PKCE is the real protection.** The client asks to be public and carries
  no confidential secret; a client secret the rare server issued is stored
  and sent, but an installed app cannot keep a secret and nothing relies on
  it.

The grant persists as `[jmap.oauth]`: the client id, the refresh token, the
discovered endpoints, the redirect URI registered (replayed verbatim on every
refresh, or the server rejects the grant), the scopes, and two fields the
chain's later RFCs add. The **resource indicator** (RFC 8707) is persisted
because every refresh must re-send it: a server that issues resource-scoped
tokens answers `invalid_target` without it, which would break the account at
its first refresh rather than at setup. The **issuer** (RFC 9207) is
persisted to check a re-authorisation's `iss` parameter against; absent means
nothing to compare, which is the pre-RFC-9207 status quo and not a reason to
break a working account.

### The token source keeps the account alive

The grant's access token is minted and kept live by the shared token source
the Graph family already rides: single-flight (one mint in flight, every
caller waits on it), replay cool-downs (a burst of concurrent 401s spends one
refresh, not one per caller), and rotation write-through. The wrapper around
the provider rebuilds the connection when the token turns, and the
capabilities are re-snapshotted from the new delegate's own session, never
carried over.

The rotation write-through is the Fastmail rule, and it binds hardest here:
a ratcheting server revokes the whole grant over a replayed refresh token,
so a rotation nobody stored kills the account at the next launch. Every
rotation the token source sees lands in the host's token sink before the
caller proceeds, and a launch that cannot write the store still connects
(read-only) rather than silently skipping the persist.

### The completion's ordering

The completion dials before it persists, and the dial-table entry is
registered first: a mid-dial rotation lands in the entry, and the final
persist reads the dial table, never the parsed copy. A grant the completion
could not dial is refused before anything is stored, so no account is left
standing on a credential the server just refused.

## The wizard's arm

The setup form asks for the one secret under a field labelled "Password or
API token", with the server URL editable and derived from the email domain
when the lookup carried none. Where the offer probe says the server
advertises discoverable OAuth, a sign-in button sits beside the field; the
button opens the authorization URL in the system browser and the completion
waits on the redirect. Closing the form while the browser is open cancels
the sign-in, so an abandoned attempt strands neither a waiter nor a
registration.

## Known gaps

- **No in-place renewal.** A JMAP account whose stored credential is refused
  shows the expired-sign-in row naming the credential, and its remedy is
  removing the account and connecting it again. An in-place re-grant that
  keeps mail, folders, and settings waits for its slice (the capabilities
  row is open).
- **A grant that dies mid-session is unobserved until the next dial.** The
  boot dial classifies a dead grant; nothing watches the session for a
  revocation that lands while the app runs.
- **Contacts (RFC 9610) ride a later slice.** The engine's JMAP contacts
  face is not yet surfaced in Astra; the session's advertisement of it
  changes nothing the product shows today.
- **The sender-name write waits for its slice.** The identity verbs the
  submission capability offers are not yet surfaced for JMAP accounts.
- **A refused re-consent can roll the dial table back past a landed
  rotation.** The re-authorisation flow's failure path restores the previous
  stored document wholesale; a rotation that landed between the flow's start
  and its refusal is lost with it. The Graph and Google families share this
  shape, and it is recorded there too.
- **IMAP/SMTP sign-in with a provider stays a separate family** (the
  mail-oauth contract); a JMAP account's OAuth is JMAP's own, discovered
  from the server, and the two never share a grant.
- **The OAuth chain's live proof pins its honest half.** The harness serves
  the RFC 9728 document (its authorization_servers names an issuer that
  resolves nowhere, the finding the provider-oauth discovery proof
  recorded), so the gated suite pins the fail-soft refusal live: no sign-in
  is offered, the password field stands. The full chain's live proof waits
  on a real JMAP identity provider, and the offline suites carry the
  complete chain over scripted servers until then.
