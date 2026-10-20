# Folder pane: the Astra Mail contract

**Scope.** The folder pane beside the mail list: the tree it draws, the roles
and badges on its rows, the unified row at its head, selection and expansion,
the edit verbs its menu offers, and the drag it accepts. Binding on every
surface that ships a folder pane. Ported in substance from mail-calendar's
folder contracts, scoped to what Astra Mail ships today; the gaps are
recorded at the end, never left silent.

**Principle.** *The pane draws the account's tree as the server holds it.*
Every edit goes through the engine's outbox and the pane re-renders over the
reconciled tree, so the pane never holds a tree of its own that could
disagree with the store.

## The tree

Every configured account stands as a group row labelled by its address (or
the name its owner sends under), its folders beneath it. A folder's row
carries its own name, never its path; the indent says where it sits. The
walk is depth-first with the role-bearing folders first in a fixed canonical
order (Inbox, Drafts, Sent, Archive, Junk, Trash), then every custom folder
by name. A role-bearing folder draws at the top of its account however the
provider files it, so Gmail's Sent never hides inside `[Gmail]`. The rows
cross the snapshot flat, one list per account, with the keys a surface needs
to select, expand, or edit.

## Roles and badges

A folder's role comes from the server's own SPECIAL-USE flags (RFC 6154),
mirrored from the JMAP equivalents; the pane never guesses a role from a
name. The unread badge is the server's own count, so it covers mail older
than the synced window and matches what another client shows. Zero draws no
badge.

## The unified row

The pane's head row, `Unified inbox`, merges every account's inbox into one
list. It renders only while the pane holds at least two accounts; one
account needs no second way to see its inbox. Clicking it selects the
all-inboxes scope: the list shows every account's inbox rows, newest first,
inboxes only, with each row bound to its own account so selection and
actions need no change. The badge sums the inboxes' unread counts. The
choice persists across launches: the scope rides the persisted UI state, so
a relaunch reopens the unified list. Search keeps its own scope; the unified
list is the list, not a search.

## Selection and expansion

Selecting a folder shows its mail; selecting an account shows that account's
whole mailbox. Expansion is deliberately not navigation: opening or shutting
a tree (an account's or a folder's) changes neither selection nor the other
trees, and the core persists the choice per account and folder so a restart
reopens the same tree.

## The edit verbs

Every tree edit is one folder intent onto the engine's outbox-mediated
mailbox write, so an edit queued offline lands when the server returns and a
refused replay surfaces as the conflict the engine answers:

- **Create.** The menu's `New Subfolder` opens an inline row under the
  folder; Enter commits, Escape and blur cancel, and an empty name never
  commits.
- **Rename.** The menu's `Rename Folder` opens an inline row over the
  folder; special folders are refused with their reason.
- **Delete.** Outside the Trash this is the recoverable verb: the folder,
  its subfolders, and their mail move to the Trash, behind a confirm.
  Inside the Trash the menu offers the permanent delete instead, behind its
  own confirm. A special folder outside the Trash never deletes.
- **Mark All as Read** loops the folder's listed unread rows when the list
  is showing the folder.

Renaming or moving re-selects the folder under the key the engine's receipt
mints for it: IMAP rename gives the folder a new key, and a selection left
on the old one would name a folder the tree no longer holds.

The verbs are gated on capability, never disabled: an account whose provider
cannot write its tree shows no edit verbs at all, so nothing offers a change
the account cannot apply.

## Drag

The tree drags with keys only; message content never rides a drag.

- A folder dropped on another folder of its own account reparents it (the
  Move verb). The descendant guard refuses a drop into the folder's own
  subtree before anything dispatches, so the tree can never land inside
  itself.
- A folder dropped on its account row becomes a root.
- A message selection dropped on a folder moves the rows in one batch, the
  same move intent the row menu's picker dispatches. The drag carries its
  shape beside its keys, so a hover refuses what the protected drag store
  hides: a conversation row refuses the whole drop, because a move batch
  can name only messages.
- Cross-account drops and the unified row refuse; a legal target highlights
  while it hovers and an illegal one does not.

## Known gaps

- **IMAP has no move, only rename**: a reparent lands as a rename of the
  whole path, and the receipt's minted key re-selection is the contract.
- **A conversation cannot be moved**: the verb disables with its reason and
  a drag holding a conversation row refuses the drop.
- **A queued folder edit shows no pending marker**: the pane draws the tree
  the store holds, and a refused replay surfaces as the engine's conflict.
- **The folder intents carry the folder key as a bare string** beside their
  `AccountId`, extending the create intent's established shape; a versioned
  `FolderRef` everywhere is a later wire decision.
