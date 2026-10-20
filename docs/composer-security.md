# Composer security: the Astra Mail contract

**Scope.** The composer's security-shaped contracts: what the document may
carry, where attachment bytes live before a send, the never-send-twice
contract, the refusals, the attachment surfaces, and the forward carry.
Binding on every surface that ships a composer. Ported in substance from
mail-calendar's compose contracts, scoped to what Astra Mail ships today;
the gaps are recorded at the end, never left silent.

**Principle.** *Nothing sends twice, and nothing sends half.* A gesture
either queues a whole message or refuses with a sentence; the words are
never spent on a send that could not happen.

## What the document may carry

The composer's document is typed blocks (paragraphs, lists, quotes, the
image references inline content names) plus attachment rows. It carries no
script, no style of its own, and no bytes: the renderer builds the MIME from
the blocks alone, deterministically, so a sent message is byte-identical to
a draft stored from the same document.

## Staged bytes behind minted handles

A picked file becomes a document row naming an opaque, freshly minted
handle; the bytes stay client-side behind it. The picker is the webview's
own multi-select input, so the bytes never round-trip through the host.
At submit the surface reads the standing rows' bytes once and hands them
back beside the document; a row removed before the send drops out of both
the document and the byte set. Nothing stages behind the composer's back:
no handle, no bytes, and no bytes without a standing row.

## The never-send-twice contract

One gesture, one send:

- The sending state is set synchronously at the click, so a second click
  in the same gesture dispatches nothing.
- The submit reads the store once at click time: the document is the one
  on screen, and the bytes are read once per gesture.
- The settlement handler stands on both chain halves, the dispatch's
  success and its failure, and lands only against its own composition: a
  submission left in flight when the pane moved on can neither close the
  next composition nor stick its error under it.
- The send itself queues through the engine's durable outbox before any
  server is asked, and the gesture drains the queue, so a send the app
  does not survive is neither lost nor delivered twice.

## The refusals

Each refusal is a sentence in the composer's own words, with the words
kept and the pane open:

- **An unreadable file**: a staged file whose bytes will not read fails
  the send with its sentence; nothing is queued.
- **A missing recipient**: the Send button stays gated with its reason
  until a recipient stands in the To field, so a blank send is never
  offered, never queued.
- **A reply without an origin**: a reply whose original arrived with no
  `Message-ID` has nothing to thread under; the surface says so instead
  of queueing a send the core would have to strand after the fact.

## Attachments

The Attach button opens the webview's file picker; each picked file stands
as a row with its name, media type, size, and a remove control. Dragging
files onto the composer attaches documents outright; a picture asks, with
one ask covering the whole drop, whether it rides inline or as a file. An
inline answer adds the image reference to the document, so the part rides
the send as body content; removing the row takes the reference with it, or
the send refuses an inline attachment nothing points at.

## The forward carry

A forward opens holding the original's attachments as removable rows. The
part ids ride the Forward intent off the reading snapshot; the core
re-reads the parts from the original at submit (never trusting bytes the
surface held), and a standing row whose part will not come back refuses
the send, the same discipline a host blob that is missing serves. A
removed row simply does not ride the send.

## Known gaps

- **Inline images are not preserved across a forward**: the carried parts
  ride as files, and the original's inline `cid:` pictures do not arrive
  as inline content in the forward.
- **Link editing, pasted-picture capture, image resize, and mailto:
  prefill** stand as future rows this document will own as they land.
