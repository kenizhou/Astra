# Reading zoom and reflow: the Astra Mail contract

**Scope.** The reading body's zoom steps and the fixed-width reflow, on
every surface that renders a message body. The gaps are recorded at the end,
never left silent.

**Principle.** *The body's document is the reader's.* The sanitised fragment
never renders bare: it rides a wrapper document the app owns, whose base
font-size is the standing zoom step, whose viewport meta is what the sender's
mobile rules read, and whose clamp rules hold every fixed-width element to
the frame.

## The three steps

Smaller, Default, Larger: 13, 16, and 20 pixels of base font-size, the same
three-step shape the app font size takes, one control in the reading header,
tracked per reader in the local appearance store beside the font-size step
(a device preference, never a mail preference that crosses accounts). The
default is the frame document's own medium, sixteen pixels, so the standing
default changes nothing the body did not already render at.

## The wrapper document

The sanitised html rides a srcdoc iframe wrapped in a document of the
reading zoom's own:

- **The viewport meta** (`width=device-width, initial-scale=1`) lets the
  sender's mobile rules read the frame's width, so a newsletter laid out
  against the reading pane's own width applies its own rules at every width.
- **The base font-size** scales with the standing step; the sanitised
  fragment rides the wrapper's body verbatim, with no escape or rewrite
  between the snapshot and the frame.
- **The clamp** holds fixed-width content to the viewport:
  `max-width:100% !important` over the body's direct children and over
  `img`, `video`, and `table`, with `img` and `video` reflowing their height,
  so a table at a hard pixel width is held to the frame and the body text
  reflows live during resize. The rules are CSS, so the browser reapplies
  them per layout pass; no script and no re-wrap runs.

## The plain leg

A plain-text body scales with the same steps and the same three values, so
one control moves every message. Unifying the legs moves the plain body's
default from 13px to 16px, the frame document's own medium.

## Known gaps

- The steps are three, matching the app font steps; a free percentage is
  not offered.
