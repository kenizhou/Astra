// SPDX-License-Identifier: GPL-3.0-only
// The message-write half of the IPC wire: the permanent delete's
// constructor. Split from ipc.ts to keep that file under the line cap, like
// the report half before it, and re-exported whole through it, so ipc stays
// the one import point of the wire. The literal is pinned in writes.test.ts
// against the JSON the webview relies on.

import type { RowRef } from './ipc';

/** The message-write family's constructors, spread into ipc's `intents` in
 *  the enum's own order. */
export const writeIntents = {
  /** Permanently delete one row (a message, or a whole conversation): the
   *  irreversible verb behind its own confirm, never a move to the trash
   *  and never a batch loop. */
  deleteMessage: (row: RowRef) => ({ delete_message: { row } }),
};
