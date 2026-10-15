A Google account can now sign in through the browser and join the
app: its mail syncs with read and actions over one account-wide Gmail
provider, sending works through the outbox, and its calendar lands on
the grid. The sign-in returns over a loopback redirect on this device
(the listener opens the port before the browser is sent to it), the
account is named from what the provider already knows (no address
typed), and the credential lives only in the device's secure store
with the non-confidential Desktop secret beside it, renewed there
should the provider ever rotate it. A sign-in that expired or was
withdrawn is reported as exactly that on the accounts page, and its
sign-in-again action runs the Google flow, not the Microsoft one. The
honest scope: the account type appears only in a build that carries
the Google Cloud registration, and no such registration ships with
this one.

Platforms: desktop
Bump: minor
