# The message own-window: the Astra Mail contract

**Scope.** A message opened in its own OS window beside the reading pane.
Binding on every surface that ships the reading view. The window follows the
composer pop-out window's pattern exactly; the gaps are recorded at the end,
never left silent.

**Principle.** *One open reading state.* The host holds exactly one open
message at a time and no per-message body command exists, so the window opens
its message through the same guarded open path the pane's row click sends and
the pane FOLLOWS the window's open: a mark-read from either reader lands
once, and the pane's snapshot shows whatever message the state holds.

## The window

A row's Open in New Window, and the reading pane's own overflow, open a
frameless, centred, resizable OS window titled by the subject. The URL
carries the message identity as its own params (the account and the provider
key, because both are opaque strings no separator can be guaranteed free of),
the folder key the list sat in (the context the window's Not-spam verb
judges), and nothing else. The window id is minted per open and the label
rides the `message-*` wildcard the window capability grants: the same
controls the composer windows carry (dragging, minimise, maximise, close,
listen) and no more.

The window renders the whole reading view the pane renders, over its own
snapshot pull: the header with its verbs, the invitation card, the AI
surfaces, the body at the standing zoom, the attachments, and the
quick-reply strip. One hydration per window: the open intent lands first, so
the reading pull that follows reads the message this window is for, then the
settings and account-tree pulls join it.

## The closing semantics

The window closes when its message closes: the open reading state no longer
holding the window's message is the closing, whichever way it happened (a
close intent, a refresh that dropped the message, another reader opening
something else). The closing arms only after the state has held the message
once (the heldOnce arming), so a broadcast that lands between the mount and
the hydration's own apply cannot shut a window that never opened.

## Outside Tauri

A plain browser holds no host to open a window through, so the verb opens
the message in the pane instead, the inline reader that is always there.

## Known gaps

- **One window at a time.** The host holds one open reading state, so
  opening a second message moves the first window to close; an independent
  per-window reading state is a later host leg.
- **No window-side navigation.** The window holds one message; moving to
  another means opening it from the list, which the pane follows.
