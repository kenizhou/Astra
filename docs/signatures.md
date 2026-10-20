# Signatures: the Astra Mail contract

**Scope.** How a signature is authored, assigned, seeded into a
composition, and swapped with the From choice. Binding on every surface
that opens a composer or edits a signature.

**Principle.** *A signature is authored once, in Settings, and never
re-typed.* The composer seeds the assigned block and treats it as retained
content the editor shows verbatim; the words a person edits in place are
theirs and are never swapped away under them.

## The authoring surface

Settings owns the editor: rich text over the same marks the composer
carries (bold, italic, underline, the font size steps, both colour marks,
links, lists). The authored block crosses as two legs, an HTML fragment
and its plain-text rendering, and persists in the preferences document
beside the engine store.

## The assignment

Each account holds two slots: the signature its new mail opens with and
the signature its replies and forwards open with. The same signature may
serve both, and either slot may stand empty while the other holds one. An
account's last empty slot takes its row with the preference file, so the
document holds only live assignments. The assignment crosses on the
settings snapshot's account row, additively; absent means the account
seeds nothing.

## The seeding

A new message seeds the new-mail slot's block at the top of the document.
A reply or forward seeds the reply slot's block directly beneath the
quoted original, before any surface reorders the quote, so the signature
stays below the person's words whatever the quote-style setting does to
the quote. A document already bearing a signature seeds nothing: a resumed
draft or a re-opened composition owns the one it has.

## The swap

Changing the account in the composer's From picker swaps a signature that
still equals the seeded baseline to the incoming account's own slot
(honouring the composition's kind: the reply slot on a reply or forward,
the new-mail slot on a plain composition), or drops it when the incoming
account seeds nothing. A signature the person edited no longer equals the
baseline and stays: an edited signature is the person's. Nothing swaps
when no baseline stands.

## The send

The send renders what the document carries: the renderer emits the block
verbatim, and the plain leg carries the RFC 3676 delimiter. No signature
is appended at send time.

## Known gaps recorded, not hidden

- An inline logo does not ride: a picture in a signature needs the
  attachment binding at seed time, which is its own change. The editor
  swallows a picture pasted into it silently, consistent with this gap.
