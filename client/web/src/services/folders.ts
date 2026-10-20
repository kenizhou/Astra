// SPDX-License-Identifier: GPL-3.0-only
// The folder half of the IPC wire: the folder intent family's carrying
// variants and the move-to-folder batch, with their constructors. Split from
// ipc.ts to keep that file under the line cap, like the sending half before
// it, and re-exported whole through it. The literals are pinned in
// folders.test.ts against the JSON astra-app's protocol_tests.rs asserts.

import type { FolderRef, MessageRef } from './ipc';

/** What the folder pane asks the runtime to do to an account's folders,
 *  nested under the `folders` tag of the intent that carries it. */
export type FolderIntentBody =
  | { create: { account: string; name: string; parent?: string } }
  | { rename: { account: string; folder: string; name: string } }
  | { move: { account: string; folder: string; parent?: string } }
  | { trash: { account: string; folder: string } }
  | { delete: { account: string; folder: string } };

/** The folder family's and the move batch's constructors, spread into ipc's
 *  `intents` in the enum's own order. */
export const foldersIntents = {
  /** Call a folder `name`, where it is staying put. */
  renameFolder: (account: string, folder: string, name: string) => ({
    folders: { rename: { account, folder, name } },
  }),
  /** Put a folder inside `parent`, or at the account's top level. */
  moveFolder: (account: string, folder: string, parent?: string) => ({
    folders:
      parent === undefined
        ? { move: { account, folder } }
        : { move: { account, folder, parent } },
  }),
  /** Put a folder, its subfolders and their mail in the account's Trash. */
  trashFolder: (account: string, folder: string) => ({
    folders: { trash: { account, folder } },
  }),
  /** Remove a folder for good: the verb the Trash's own menu offers. */
  deleteFolder: (account: string, folder: string) => ({
    folders: { delete: { account, folder } },
  }),
  /** Move a set of messages to one folder of their own account. */
  moveMessages: (rows: MessageRef[], folder: FolderRef) => ({
    move_messages: { rows, folder },
  }),
};
