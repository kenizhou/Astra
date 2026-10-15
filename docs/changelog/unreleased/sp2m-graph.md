A Microsoft account can now sign in through the browser and join the
app: its mail folders sync with read and flag actions, sending works
through the outbox, and its default calendar lands on the grid. The
sign-in runs in the person's own browser session, the account is named
from what the provider already knows (no address typed), and the
credential lives only in the device's secure store, renewed there
whenever the provider rotates it. When a permission goes missing the
account says so on the accounts page and offers to sign in again; a
sign-in that expired or was withdrawn is reported as exactly that,
never as a server outage. The honest scope: the account type appears
only in a build that carries the Microsoft app registration, and no
such registration ships with this one.

Platforms: desktop
Bump: minor
