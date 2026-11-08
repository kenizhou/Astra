A message opens in its own window from the row menu or the reading pane's
overflow: the whole reading view in a frameless, resizable OS window that
closes when the reading moves off its message, with the pane following the
window's open and a mark-read landing once whichever reader opened it. The
overflow also gains Save as .eml, which writes the message exactly as
delivered (the engine's own source bytes, folded headers and transfer
encodings intact, never a re-serialisation) through the platform save
dialog with a subject-derived name, and Print, which renders a
print-optimised document (the subject, the header lines, the sanitised
body, the attachment names) and opens the platform print dialog over it,
closing once the dialog settles.

The reading body gains a zoom control: Smaller, Default, and Larger move
the base font-size inside the sanitised body's own document, whose viewport
meta now lets a newsletter's mobile rules read the pane's width and whose
clamp holds fixed-width tables and images to the frame so the text reflows
live during resize. Unifying the plain-text body onto the same steps moves
its default from 13px to 16px, the size the HTML body always rendered at.
The zoom and the position of the pane are per-device choices.

Mark as spam in the row menu and Not spam in the reading pane become real:
one report that both tells the provider and files the message (into Junk,
or back to the Inbox), so there is no second move and no separate undo; the
opposite verb is the remedy. Message rows gain touch swipes: swipe right
for the primary action and left for the secondary, configurable per
direction in the Reading settings, with an undo toast that moves the
message back.

The batch is proven live against the local harness: a message with folded
headers and a quoted-printable body is exported byte for byte against a
fresh source read, a junk report files its note into the Junk folder on the
real server and the correction files it back, and a fixed-width table keeps
its hard pixel width through the pipeline into the body the clamp holds.

Platforms: desktop
Bump: minor
