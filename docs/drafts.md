# Drafts: the Astra Mail contract

**Scope.** What happens from the first pause in a composer to the draft's
return: storing the words while you type, resuming what was stored (the
account's latest copy at app start, the row you clicked in Drafts), the
adoption that makes the next save replace rather than add, the files the
draft carries, and the row's own verbs. Binding on every surface that
ships a composer. The send side of a draft (what a submit does with the
stored copy) is sending.md's; the staging of new files is
composer-security.md's.

**Principle.** *One copy per message in progress.* Every save of one
composition carries the same `Message-ID` and names the copy it replaces,
so the Drafts folder holds one draft per message being written, never a
row of near-identical fragments beside each other.

## Storing while you type

After a pause in the composer (thirty seconds untouched; the trigger is
the pause, not the clock, so someone still typing is never made to wait on
an upload) the save renders the composer's document through the same
deterministic renderer a send uses and stores it in the account's Drafts
folder through the engine's outbox-mediated put: recorded durably before
any server is asked, applied at once when the network is there, parked and
retried when it is not.

The put names the copy it replaces. The provider answers with the key the
draft is stored under now, and that answered key (not the one that went
in; most adapters move it on a re-save) is what the next save passes as
its replacing target, so an edited draft costs the server one write
rather than one per save. A save whose content is unchanged writes
nothing at all and still reports saved, because what the person asked to
keep is on the server. Closing the composer keeps the stored copy; a
final save runs under the same gate and the composition is forgotten.
Discarding removes the stored copy and withdraws any save still parked in
the outbox first, so the words the person threw away cannot arrive after
the composer has gone.

## Resuming

Two roads open a stored draft. At app start the account's latest stored
draft resumes into a fresh composer. In the Drafts folder the row you
clicked resumes by its own account and key, so a folder of drafts reopens
the one you named rather than the newest.

The by-key read refuses and adopts nothing on three gates: the key
resolves to nothing, the message is not a draft, or its body cannot be
read. A composition that opened without the draft's content must not be
one whose next save replaces it, so nothing is adopted in any of those
cases and the composer opens on the blank tail instead.

The adoption is what makes the reopen safe: the composition takes the
stored copy's key (its next save supersedes that copy, a discard removes
it) and its `Message-ID` (every save of the composition keeps naming the
one message). No unchanged-content digest is recorded across the resume,
because the body the composer opens with is text derived from the stored
draft: the first save after a resume always writes.

## The carried parts

The files the stored copy carries come back as pre-attached removable
rows in the forward's own shape: metadata and a blob handle naming the
part, never bytes and never a `data:` URI. The bytes are read once
core-side at the resume and parked on the composition's record, so
neither a later save nor the send has to name the server again; the bytes
never cross to the host and back.

A row the composer removed drops its handle, so its file stamps neither
the stored copy nor the send. The rows that still stand stamp into the
stored MIME on every save, which is what keeps a draft's files across
reopen after reopen. A submit resolves the standing rows' bytes from the
parked parts core-side and refuses a handle with no parked part (the
forward's own sentences), so nothing is sent with a file missing.

## The row verbs

The Drafts row's menu carries three verbs over its key. Open resumes the
named copy into the composer pane; Open in New Window resumes it into a
composer window of its own. Delete Draft removes the stored copy through
the engine's outbox (queued when the server is away), under the same save
gate as the composition discard so a save in flight cannot slip its put
between the withdrawal and the delete and resurrect the row; a save still
parked under the same `Message-ID` is withdrawn first.

## A composer still open over a row-deleted draft

Deleting a Drafts row does not close composers. A composer still open
over the deleted copy keeps its words: an unchanged save is a no-op (the
composition's record still says those words are saved), and an edit
re-stores the words as a new copy. That new copy is correct, not a
residue: the words are on the person's screen, and a draft of them
belonging to nobody would be the loss.

## Known gaps

- **The stored copy's `Bcc`.** A `Bcc` is not in the bytes most transports
  hand back, so a draft saved elsewhere usually resumes without one.
- **A resumed reply degrades to plain mail.** The compose request is
  session state and the draft save carries no threading, so a reply
  resumed after an app restart sends as a new message; the full shape is
  sending.md's gap, recorded there.
- **A carried picture row shows a placeholder until it renders.** A
  resumed draft's picture rows carry handles, not bytes, so the picture
  surface falls back to its placeholder row for them; the composer
  security document records the edge.
