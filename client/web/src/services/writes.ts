// SPDX-License-Identifier: GPL-3.0-only
// The message-write half of the IPC wire: the permanent delete's and the
// stored draft discard's constructors. Split from ipc.ts to keep that file
// under the line cap, like the report half before it, and re-exported whole
// through it, so ipc stays the one import point of the wire. The literals are
// pinned in writes.test.ts against the JSON the webview relies on.

import type { MessageRef, RowRef } from './ipc';

/** The message-write family's constructors, spread into ipc's `intents` in
 *  the enum's own order. */
export const writeIntents = {
  /** Permanently delete one row (a message, or a whole conversation): the
   *  irreversible verb behind its own confirm, never a move to the trash
   *  and never a batch loop. */
  deleteMessage: (row: RowRef) => ({ delete_message: { row } }),
  /** Remove one stored draft the Drafts row names, with no composition to
   *  forget: the row's own Delete Draft verb, queued on the host when the
   *  server cannot be reached. */
  discardStored: (message: MessageRef) => ({
    drafts: { discard_stored: { message } },
  }),
};
