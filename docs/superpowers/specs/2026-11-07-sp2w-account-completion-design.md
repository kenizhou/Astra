# SP2w design: the account completion batch

**Status.** Decided 2026-11-07. Slices branch from main at this commit.

**Scope.** The account rows the matrix still holds unstarted: the outgoing
name per account, the JMAP sign-in renewed in place, and the multi-account
switcher; plus the vendor-commerce family's honest resolution (the
explicit N/A the backlog pre-authorized). The engine ships every verb
this slice wires (the sender identity verbs, unused; the account-layer
reauth arms, ported at SP2o with no caller); the switcher needs only a
surface affordance.

## D1: the outgoing name

Astra asks the person's name once, at connect, where the account has
none: after the account joins, the wizard (or the accounts page for an
account added headlessly) offers the name field. Where the provider
already knows a name (the engine's `sender_identities` answers one), the
account adopts it silently and the settings show it; where the provider
can take a write (`IdentityControls::Writable`: Google, JMAP), the
settings card edits it and pushes best-effort; where it is read-only
(Graph) the card shows it with the reason; where the transport owns none
(IMAP), the name is a local preference the account stores and the From
line carries. The name never validates client-side beyond length (the
engine's bound); the empty name means "the address alone". The From
picker and the sent mail carry the name; the account rows show it beside
the address.

## D2: the JMAP re-grant in place

The JMAP expired row's remedy becomes the re-grant: the row's action
opens the re-authorisation from the persisted grant (the account-layer
`begin_jmap_reauth`, no re-registration), the completion swaps the
grant in place through the credential transaction (keeping mail,
folders, and settings; refusing a different-address completion per the
standing rule), and the account re-dials without losing anything. The
IMAP/SMTP OAuth family shares the shape where its grant stands (the
same session machinery); the password family's expired row keeps its
reconnect remedy (a password has nothing to re-grant).

## D3: the account switcher

The unified row gains its sibling: selecting an account from the folder
pane's account header (a click on the account's name row where the
expansion toggle stands today, keeping the chevron for expansion)
selects the account's scope (the `SelectAccount` verb the core already
holds), the choice persists (the shape preference's selected-scope
discipline from SP2p), and the list header shows whose mail stands (the
account's name, or All Accounts). The switcher is the scope verb's
surface, nothing more.

## D4: the vendor family's resolution

The three commerce rows (the next-device account list, the vendor
sign-in, the subscription view) record the explicit N/A the backlog
pre-authorized: the client contract lives in mail-calendar's
`allodia_license/` under LicenseRef-Allodia-1.0, whose §4.2 forbids
reuse in another product (the code is published to be audited, not
reused), and the business behind it is Allodia's closed account service.
A port would be a from-scratch commercial service build, not a port;
the rows move to their decided state naming both the license bar and
the owner's from-scratch path, never standing silent.

## Known gaps recorded, not hidden

- The name push is best-effort: a provider that refuses the write keeps
  the local name and shows the refusal sentence.
- The vendor family's N/A is a decision, not a shortfall: reopening it
  means the owner building a vendor service.
- The switcher selects scopes; it does not reorder or pin accounts.

## Test-first

Every behaviour lands with its red: the adopt/set/keep-local rules over
the engine's real verbs per family; the wizard's ask-once; the settings
card's three controls (writable, read-only, local); the re-grant's
in-place swap and its different-address refusal; the switcher's scope
selection and persistence; and the live proofs: a Google-styled
identity adopt and a JMAP name write observed over the harness where
the transports admit it (the harness's JMAP Identity/get), and the
re-grant's grant swap observed in the stored document.
