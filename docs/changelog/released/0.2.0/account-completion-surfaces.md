The account completion surfaces: the wizard asks the outgoing name once
after an account connects (closing the step cleanly where the provider
already answered one, even when that answer arrives mid-step), and the
accounts page carries the same ask for accounts added without the
wizard, beside the name's three controls: an editable name pushed back
to the server where the account holder owns it, a read-only name with
its reason, and a locally kept name for transports that have none. The
folder pane's account headers and the composer's From picker carry the
name beside the address.

A JMAP account whose sign-in expired can be renewed in place from its
accounts page row: the re-grant re-runs the same authorization against
the stored grant and swaps the fresh one in, keeping the account's
mail, folders, and settings. An IMAP/SMTP account signed in with a
provider keeps the reconnect remedy for now; its grant shares no
re-authentication arm with the JMAP family.

Selecting an account's name row in the folder pane now shows that
account's mail in the list, the choice remembered across launches, with
the list header naming whose mail stands (the account's name, or All
Accounts).

Platforms: desktop
Bump: minor
