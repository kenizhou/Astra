Mail now arrives while the app runs. One delivery loop per dialled
account starts beside the boot sync, joins it, and then watches or polls:
IMAP accounts idle on the INBOX where the server offers it (behind the
engine's connection-headroom guard), JMAP accounts follow the server's
event stream, and everything else polls the account's own cadence, 15 to
120 minutes and 30 by default, picked up from the settings without a
restart. Every pass the loop lands goes through the same single-account
sync path a manual refresh takes, so snapshots, the outbox, and the
warm-body pass land identically. A lost watch records its failure, backs
off from one second doubling up to the cadence, and syncs before it
watches again.

New mail announces itself. Each background pass collects the messages
that are new, in an inbox-class folder, and not the account's own sent
filing; while the main window is unfocused each one raises an OS
notification naming who it is from, what it is about, and how it begins,
more than three in a pass raise a single summary instead, and nothing at
all shows while the window is focused. A settings switch (on by default)
turns notifications off; the switch lands in the settings screen with
the settings slice.

Connectivity state reaches the surfaces. Two consecutive network-class
failures mark an account in outage and any successful pass clears it;
the offline banner answer and the per-account outage list now ride the
sync-progress snapshot to the web, republished the moment they move, so
the banner and the pane badges (their rendering lands with the surface
slice) always read the latest state.

The batch is proven live against the local harness. A mail submitted
while the recipient's INBOX watch stands answers `Changed`, the pass it
runs lands the mail, and the new-mail queue holds the notice; a poll
arm's pass lands a delivered mail through the same seam beside the
cadence the settings carry; the body cap's states run over the
account's own passes (off keeps the notes as metadata alone, a byte
cap warms them until none is missing its body, off again drops the
cached sources with the reading pipeline still answering); and the
outage classification runs over a real refused dial, two refused passes
marking the only dialled account and one successful pass clearing both.

Platforms: desktop
Bump: minor
