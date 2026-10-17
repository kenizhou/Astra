# SP2p design: the mail batch

**Status.** Decided 2026-10-17. Slices branch from main at this commit.

**Scope.** The mail batch the backlog names: folder management (create,
rename, move, delete, drag), attachments both ways, multi-select actions,
the unified All Accounts group, and move-to-folder. The engine already
ships every verb this slice wires (edit_mailbox with create, update, trash,
delete; MailEdit::MoveTo; message_attachment byte fetch; the composer's
blob handles); Astra consumes none of them today. This slice is wiring and
surfaces, with the live proofs and the docs the rows cite.

## D1: folder management through the app

FolderIntent grows from its one refused Create to the engine's four verbs:
Create, Rename (Update carrying the new name), Move (Update carrying the
new parent), Trash, and Delete (permanent, offered only inside Trash or
after a confirm). Dispatch maps each onto the engine's outbox-mediated
edit_mailbox with a minted idempotency key, so a folder edit queued offline
lands when the server returns. The account's capability snapshot gates the
surface: a provider without mailbox_writes shows no folder edit verbs,
never a disabled menu item that fails on click. Delete goes to Trash first
(the Trash verb); permanent Delete is the verb the Trash's own menu offers,
behind a confirm dialog. Renaming or moving re-selects the folder under its
returned key (the receipt names the folder the edit resolved to; IMAP
rename mints a new key).

## D2: the unified All Accounts group

Scope::AllInboxes gains rows: the snapshot merges every account's inbox
rows, newest first, the same row shape the per-folder list renders (RowKey
is account-bound by design, so selection and actions need no change). The
pane's unified row becomes live: clicking it selects the scope, the unread
badge it already renders keeps counting, and the choice persists across
launches (the scope rides the persisted UI state the settings slice
established). Search keeps its own scope; the unified list is the list, not
a search.

## D3: move to folder

A MoveTo intent (a row set, a destination folder key, one account: moves
stay within the account, the row's own constraint). The row menu's
placeholder becomes a submenu of the account's folders (the mover
accounts), and the multi-select batch loops it exactly as the other batch
actions loop theirs. The engine's MailEdit::MoveTo does the work through
the existing outbox path; the destination is the folder's key the pane
projects, and a move to the folder the row already sits in is a no-op the
app answers without queueing anything.

## D4: folder drag

The folder tree grows HTML drag: a folder onto another folder of the same
account reparents it (Move with the descendant guard: a folder cannot land
inside its own subtree, the app refuses before anything queues), a folder
onto its account row makes it a root, and a message selection onto a folder
moves the rows (D3's intent, the same loop). The drag carries only keys,
never message content; the drop target highlights while a legal target
hovers and refuses while an illegal one does (cross-account, into a
descendant, onto the unified row).

## D5: attachments both ways

The read side: the reading pane's Save As and Download All become real. A
SaveAttachment intent carries the row reference and the attachment id; the
app core reads the bytes through the engine's message_attachment; the
desktop host answers with its save dialog and returns the path; the web
surface shows the saved path or the refusal. The composer side: an attach
button opens the file picker (the webview's own multi-select input, so the
bytes never round-trip through the host), each picked
file becomes a DraftAttachment carried in the document, the actions row
lists them with sizes and a remove control, and the send carries the blobs
(the wire and the core's resolution already ship; the composer stops
sending blobs: []). A forward opens holding the original's attachments
removable one by one: the forward intent gains the original's attachment
metadata, the composer renders them as pre-attached rows, and a removed
row simply does not ride the send. Drag files onto the composer attaches
them (a document attaches, a picture asks inline or file).

## D6: multi-select completion

The batch actions the row menu already loops (read, unread, flag, archive,
delete) are joined by trash-versus-delete honesty: Delete in a folder is
move-to-trash, and the Trash's own menu offers the permanent delete. A
bulk toolbar appears while a selection stands (the count, select all,
clear, and the actions), so the lot does not depend on the context menu.
The invitation's own .ics part stops listing as an attachment when the
message is an invitation (the part the invitation surface owns), while a
genuinely attached calendar file still lists.

## Known gaps recorded, not hidden

- Folder drag over IMAP reparents by rename (the protocol's only move);
  the receipt's key re-selection is the contract.
- An invitation that carries a second, distinct attached calendar file
  loses that chip too: the hide rule discriminates by media type because
  no part identity crosses the engine's scheduling read, and
  part-identity discrimination is a later engine surface.
- The folder intents carry the folder key as a bare string beside their
  `AccountId`, extending the create intent's established shape; a
  versioned move to `FolderRef` everywhere is a later wire decision.
- Attachment preview opens only what the host shell can open (the saved
  path); no in-app preview rides this slice.
- The unified list holds inboxes only (the scope's own name); a
  configurable unified set is a later row.
- Offline folder edits queue behind the engine's outbox; a queued edit's
  menu shows the pending state, and a refused replay surfaces as the
  conflict the engine answers.

## Test-first

Every behaviour lands with its red: the app core's folder dispatch and
unified rows over fakes, the move and batch intents, the attachment save
and composer blobs over the harness's byte paths, and the live proofs (T4)
against the running harness: folders created, renamed, moved, trashed, and
deleted over IMAP; a move-to-folder live; an attachment round trip (attach,
send, receive, save); the unified rows across both harness accounts.
