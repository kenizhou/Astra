// SPDX-License-Identifier: GPL-3.0-only
// The surface snapshot types, mirroring astra-viewmodel's serde wire field for
// field: an option the Rust side skips serialising is optional here too, and a
// field it always writes is never optional. Field names stay on the wire's
// snake_case so the types read exactly what crosses the IPC.

/** The special role a folder plays, crossing as a snake_case word. */
export type FolderRole =
  | 'inbox'
  | 'drafts'
  | 'sent'
  | 'archive'
  | 'junk'
  | 'trash'
  | 'other';

/** One sidebar folder row, already depth-first ordered within its account. */
export interface FolderRow {
  /** The mailbox's provider key; stable identity, used to select it. */
  key: string;
  /** The folder's own display name, never the path to it. */
  name: string;
  /** The folder's special role; absent for an ordinary custom folder. */
  role?: FolderRole;
  /** How many messages the server counts as unread; zero draws no badge. */
  unread: number;
  /** The key of the row drawn above this one; absent at the top of the tree. */
  parent?: string;
  /** How many indent steps deep the row sits; zero at the top. */
  depth: number;
  /** Whether any other row names this one as its parent. */
  has_children: boolean;
  /** Whether this folder's own children are showing; the core's own answer. */
  expanded: boolean;
}

/** One account's group in the folder pane. */
export interface AccountFolderRow {
  /** The account's stable id. */
  account_id: string;
  /** The label the pane shows on the group header. */
  display_name: string;
  /** Whether this account's folder tree is open in the sidebar. */
  expanded: boolean;
  /** The account's sorted folder rows, ready for the sidebar. */
  folders: FolderRow[];
}

/** The mailbox-list snapshot the pane renders after every pull. */
export interface MailboxListSnapshot {
  /** Every account's sorted folder list, one entry per configured account. */
  account_folders: AccountFolderRow[];
}

/** One account's position between sync passes, crossing as a snake_case word. */
export type AccountSyncState = 'syncing' | 'idle';

/** One account's position within the sync-progress snapshot. */
export interface AccountSyncProgress {
  /** The account this entry reports on. */
  account: string;
  /** Whether a pass is running for the account, or its last pass has ended. */
  state: AccountSyncState;
  /** How far the account's pass is, when a position is known. */
  detail?: string;
}

/** The sync-progress snapshot the status bar renders after every pull. */
export interface SyncProgressSnapshot {
  /** One entry per account with a sync position to report. */
  accounts: AccountSyncProgress[];
}
