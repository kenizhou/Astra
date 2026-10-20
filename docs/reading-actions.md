# Reading actions: the Astra Mail contract

**Scope.** The reading pane's actions over a received message: Save As,
Download All, the typed outcome the pane shows, the log contract, and the
`.ics` rule that keeps an invitation's own part out of the attachment list.
Binding on every surface that ships a reading pane. Ported in substance
from mail-calendar's attachment contracts, scoped to what Astra Mail ships
today; the gaps are recorded at the end, never left silent.

**Principle.** *The bytes live with the engine until a save names them.*
The pane lists metadata only; the read happens at the save, through the
core, over the engine's cache-first fetch.

## Save As

One row, one command chain: the app core reads the part's decoded bytes
through the engine, the host asks its platform save dialog where to put
them (the row's file name as the suggestion), writes the file, and answers
the typed outcome. **Download All** loops the same chain over every row;
the outcome line answers the last row that reports.

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
- **Save as .eml and Print** stand as future rows this document will own
  as they land.
