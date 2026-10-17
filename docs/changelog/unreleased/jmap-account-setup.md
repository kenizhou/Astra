Account setup now offers a JMAP route end to end: a detected JMAP
server continues onto its own form that asks for one secret, labelled
"Password or API token" (a login password, an app password, or an API
token all work, because the server itself says which scheme it wants),
with the server URL shown editable and derived from your email domain
when the lookup carried none. Where the server's own 401 advertises
discoverable OAuth, a "Sign in with your provider" button sits beside
the field; any discovery miss leaves the field standing, never an
error. The account connects at launch, joins the accounts page under
its own JMAP kind, and serves every folder through one account-wide
connection. A JMAP account whose stored credential is refused shows the
expired-sign-in row naming the credential, with reconnecting the
account as its remedy.

Platforms: desktop
Bump: minor
