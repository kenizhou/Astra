# Avatars: the Astra Mail contract

**Scope.** The one avatar every surface shares: the monogram, the photo, the
cache, and what an avatar costs. Binding on every surface that draws a person.
The monogram's derivation and the photo's path live in `contacts.md`; this
document owns where the avatar stands and what it costs.

**Principle.** *One derivation, one command, no blocking.* The core derives
every monogram once, the photo crosses only through the validated cached-photo
command, and no surface ever waits on a photo to render what it already holds.

## The monogram

The monogram is the core's one derivation (`contacts.md` owns its rule: the
first character of the first and last words of the person's best name, on a
palette colour keyed by the address). A surface never derives its own initials
or colour, so the same person cannot read as `AL` on one screen and `Ada` on
another. With no initials to draw, the circle wears the person glyph.

## The photo

The address-book photo replaces the monogram when one stands. It is read from
the engine's cached contact-photo store through the host's validated photo
command, which serves a data URI for paths inside the engine's own
contact-photo area: the web view never reads the file system, and no photo
path crosses as a URL a browser could be pointed elsewhere by. The monogram
stands until the photo lands; a row, a header, or a list never blocks on the
read, and an unresolvable read leaves the monogram standing.

The cache's trust bounds are the engine's. A resource the provider answered
"no photo" for is trusted a week before anyone asks again (a correspondent
outside the address books has no picture anywhere, and re-probing every
stranger every pass costs a request per stranger); a photo whose card carries
nothing that would notice a change (no media fingerprint, no revision token)
is trusted three days and re-fetched after; and a card that does carry a
fingerprint or a revision invalidates its photo on its own, so a re-fetch
happens only when it can learn something new. An inline photo (the vCard's
own `PHOTO;ENCODING=b` bytes, stored as a data URI) is its own revision: the
bytes are the picture.

## The cache

One ask per address per session: the web service holds the resolved view
(monogram and photo path) per address, so a pane that remounts redraws from
memory instead of crossing the wire again. A refused ask is dropped rather
than cached, so a later mount retries.

## Where the column stands

- **The mail list row** draws the sender's avatar at the leading edge,
  resolved per address through the cache. The column hides at the compact
  density (the row's own density rules), because a 44 pixel row trades the
  circle for the words.
- **The People view** and the **composer's suggestions** draw the avatar from
  their own snapshot rows and the same command.
- **Reading surfaces** (the reading header) resolve by address alone through
  the same command.

## The cost note

The photo is re-read per address per session: the cached file's bytes ride
the command's answer as a fresh data URI per ask, so a list of photo-bearing
senders costs one file read per address per session. A batched resolve (one
ask for a screenful of addresses) is the later row this note records.

## Known gaps

- **No batched resolve.** Each address resolves on its own ask; a screenful
  of photo-bearing senders costs a file read per address.
- **No avatar edit.** Uploading or cropping a photo is a contacts write this
  slice does not carry.
