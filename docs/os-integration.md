# OS integration: the Astra Mail contract

**Scope.** The three legs where Astra Mail meets the operating system:
the default-mail offer, the main window's remembered geometry, and the
share hand-off into a new message. Binding on the desktop host. The
tray and autostart stay absent; no row names them.

**Principle.** *The OS is asked, never grabbed.* Every registration is
an explicit acceptance, every remembered shape is a saved truth the
next launch restores or honestly drops, and everything the OS hands
the process is filtered at the door before it reaches a message.

## The default-mail offer

Astra Mail offers to become the OS's default mail app once, and the
offer is changeable in settings.

- **Ask once.** The ask-once latch is host-side, a `default-mail-asked`
  file in the app data directory, because the acceptance is an OS
  registration the host owns and the core's settings block has no
  other use for. Both answers to the ask spend it: the acceptance
  registers, the pass takes the banner down. The offer stands only
  over an account (no account, nothing to offer) and a refused latch
  read never nags: an unreadable latch counts as asked.
- **The settings change.** The General page's *Make Astra Mail the
  default mail app* row re-asserts the registration through the same
  verb the offer's acceptance uses, so the default is changeable after
  the ask. The verb is idempotent and honest: a registration that
  already stands answers `already_default` without touching the OS,
  because the person may have re-pointed the default at another app in
  between, and only their own ask moves it back; a refusal answers
  with its own sentence, ready to show.
- **The registration itself.** The acceptance claims the `mailto`
  association through the deep-link plugin's runtime registration, the
  same registration the installed build's manifest declares
  statically. The OS-level proof that the hand-off took (the OS
  handing a real mailto link over) waits on an installed build,
  exactly as the mailto row already records.

## The window state

The main window remembers size, position, and maximised across
launches, hand-rolled over the app data dir (`window-state.json`, no
plugin), because the window-state file is not a preference: the core's
preferences document stays the one document it is.

- **Size and position apply at creation.** The setup hook applies the
  persisted geometry after the shell creates the window and before the
  event loop's first paint, so no wrong-size frame ever flashes.
- **The maximised state applies after the frame shows.** The page's
  first finished load is the moment, because maximising a window
  before its frame shows clobbers the restore-down geometry (the flash
  this one-shot exists to avoid).
- **The monitor intersection.** A position saved on a monitor that no
  longer exists (a laptop leaving its dock) would open the frame
  invisible: the saved rectangle must touch a standing monitor, or the
  config defaults stand instead. A monitor list that cannot be read
  counts as touching, because refusing to restore over a read refusal
  strands a saved state that may be perfectly fine.
- **The fallback.** A missing, unreadable, unparseable, or insane file
  falls back to the config defaults (the fixed 1200 by 800 the window
  config declares), which is also what a first launch stands at. A
  window nobody could size or place is a corrupt file, not a state to
  restore.
- **The restore-down limitation.** The OS's restore-down rectangle is
  not readable through the shell's API, so a maximised close saves the
  maximised frame's own geometry: a window closed maximised restores
  maximised, and a later unmaximise stands at the maximised frame's
  size. The named cost of the hand-rolled state, accepted over a
  geometry guess.

The close saves best effort: a geometry that cannot serialise or
persist logs, never fails the close that asked for it.

## The share hand-off

Files the OS or a person hand the process cross into a compose prefill
through one event, the mailto channel's sibling.

- **The Windows judgement.** The OS share contract (the Share UI a
  packaged app registers as a target for) needs package identity, and
  the desktop build is an unpackaged Win32 app, so no runtime
  registration can make it a share target. The honest hand-off that
  ships is the file-association *open with* road: file arguments reach
  the process at launch, or through the single-instance guard when the
  app already runs, exactly the argv roads a mailto link takes.
- **The door.** Everything is filtered before it crosses: only
  existing plain files with plain names survive. Directories, paths
  that name nothing, names that are not plain (empty, the two names a
  path resolves on its own, any control character) are dropped, and so
  is every type the OS would execute on a double-click, the script
  hosts a stock install associates included (`exe`, `bat`, `cmd`,
  `com`, `scr`, `msi`, `ps1`, `vbs`, `js`, `jse`, `vbe`, `wsf`, `wsh`,
  `hta`, `lnk`, `pif`, `scf`, `cpl`, `reg`, `jar`): an executable or a
  script mailed through a share is the classic phishing shape.
- **The pending-row UX.** The survivors cross on the share channel and
  the composer opens a new prefilled message with one pending row per
  file. The row shows the file's own name; the path rides the
  composition held and never renders.
- **The byte-staging remainder.** No read seam ships: the webview
  holds no file system of its own, and the pending rows stand with
  their bytes unstaged, so the send refuses honestly while any stands
  rather than mailing a promise.
- **The packaged share target remainder.** Registering as a real OS
  share target needs the packaged build; the row carries it as the
  named wait.

## Known gaps

- The default-mail registration's OS-level proof waits on an installed
  build (the mailto row's standing wait).
- The share target is the open-with road, not the OS Share UI; the
  packaged build is the wait.
- Shared files stage no bytes: the pending row names the file and the
  send refuses while one stands.
- The maximised close's restore-down geometry is the maximised frame's
  own, the named cost above.
