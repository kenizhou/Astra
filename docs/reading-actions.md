# Reading actions: the Astra Mail contract

**Scope.** The reading pane's actions over a received message: Save As,
Download All, Save as .eml, print, the typed outcome the pane shows, the
log contract, and the `.ics` rule that keeps an invitation's own part out of
the attachment list. Binding on every surface that ships a reading pane.
Ported in substance from mail-calendar's attachment contracts, scoped to
what Astra Mail ships today; the gaps are recorded at the end, never left
silent.

**Principle.** *The bytes live with the engine until a save names them.*
The pane lists metadata only; the read happens at the save, through the
core, over the engine's cache-first fetch. The whole message is no
different: the `.eml` is the engine's own source read, never a
re-serialisation.

## Save As

One row, one command chain: the app core reads the part's decoded bytes
through the engine, the host asks its platform save dialog where to put
them (the row's file name as the suggestion), writes the file, and answers
the typed outcome. **Download All** loops the same chain over every row;
the outcome line answers the last row that reports.

## Save as .eml

The overflow's Save as .eml writes the engine's `message_source` bytes:
the raw RFC 5322 source the sender's server delivered, exactly as the
engine holds it. A re-serialisation would change the wording of headers,
the order of parts, and the transfer encodings, so none runs: folded
headers stay folded, a quoted-printable body stays encoded, and the
server's own prepended headers ride along.

The suggested name derives from the subject: every character a file name
cannot carry (path separators, the characters Windows forbids, control
characters) folds to a space, whitespace runs collapse to one, the stem
sheds trailing dots and spaces and clamps at eighty characters, and the
`.eml` suffix lands. A subject that sanitises to nothing, no subject at
all, or one whose stem is a Windows reserved device name (`CON`, `NUL`,
the COM and LPT ports, whose dotted forms the write would refuse)
suggests `message.eml`.

The command answers the same typed outcome family the attachment save
answers, through the same save-dialog seam: the saved path, the reader's
own cancel, or the refusal's sentence.

## Print

The overflow's Print opens a print-optimised window over the open message
and prints itself: `window.print()`, the platform dialog the webview
opens. A host print verb exists in Tauri 2.12 but on Windows it is
`window.print()` evaluated by the host, the wrapped JS API exposes none,
so keeping print in the web adds no permission.

The document is the sanitised reading view reused verbatim, never a second
sanitiser: the subject, the header lines (the sender's name, address, and
date), the body at the standing zoom, and the attachments as names. The
body rides the reading pane's own instrument, a sandboxed frame over the
sanitised fragment with the gated content policy, so the sender's styling
renders and no script and no remote load ever crosses print.

The window prints once the document stands, then closes when the dialog
settles (`afterprint`); Escape closes it when no dialog came at all.

## The typed outcome

The command's answer is one of three shapes, never a rejected call:

- **Saved**: the bytes landed at the path the reader chose; the pane shows
  the path.
- **Cancelled**: the reader closed the dialog without choosing; nothing
  was written and nothing is shown.
- **Failed**: the refusal's own sentence (the read's or the write's),
  ready to show in the pane.

## The log contract

Nothing about the read is logged beyond the path the reader chose: the
account, the message key, and the suggested name stay out of every log
line.

## The `.ics` rule

A message the invitation surface claims drops its calendar parts from the
attachment rows: the card renders the meeting, so the `.ics` it came from
is not also a file to download. A calendar file an ordinary message
carries keeps its row.

The hide rule discriminates by media type, because no part identity
crosses the engine's scheduling read: both `text/calendar` (the iMIP type)
and `application/ics` (the belt-and-braces copy some senders attach) hide
on a claimed invitation. The corner that follows is recorded, not hidden:
an invitation that carries a second, genuinely attached calendar file
loses that chip too, and part-identity discrimination is a later engine
surface.

## Known gaps

- **No in-app preview**: the Preview item stands as a placeholder, and
  opening an attachment means the saved path through the host shell; no
  preview rides this slice.
- **Print prints the sanitised reading view**, never the raw source; a
  print original is not offered.
