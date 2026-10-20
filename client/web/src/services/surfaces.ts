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
  /** The account's sending address, the field a From picker offers. */
  address: string;
  /** Whether this account's folder tree is open in the sidebar. */
  expanded: boolean;
  /**
   * Whether the account's provider can change its folder tree: what decides
   * whether the pane offers the folder edit verbs. Absent reads as false, so
   * an incapable account never shows the verbs.
   */
  can_edit_folders?: boolean;
  /** The account's sorted folder rows, ready for the sidebar. */
  folders: FolderRow[];
}

/** How the mailbox list is grouped, crossing as a snake_case word. */
export type ViewMode = 'flat' | 'threaded';

/**
 * The identity a mailbox-list row is matched by, crossing externally
 * tagged: the message or thread it shows, bound to the account holding it.
 * A provider key or thread id is unique only within its account, so the
 * account is part of the key.
 */
export type RowKey =
  | { message: { account: string; key: string } }
  | { thread: { account: string; thread_id: string } };

/** One mailbox-list row: a message in flat mode, a conversation in threaded. */
export interface MessageRowView {
  /** The row's identity: what an action on the row names. */
  key: RowKey;
  /** The account the row's mail belongs to, carried beside the key. */
  account_id: string;
  /** The sender's display name, empty when the header carried none. */
  sender_name: string;
  /** The sender's bare address, empty when there is no sender. */
  sender_address: string;
  /** The subject, empty if none. */
  subject: string;
  /** When the message was sent, as a whole-second UTC instant string. */
  sent_at: string;
  /** Whether the message is unread. */
  unread: boolean;
  /** Whether the message is flagged. */
  flagged: boolean;
  /** Whether the message has a non-inline attachment. */
  has_attachments: boolean;
  /** How many messages the conversation holds; absent on a flat row. */
  thread_count?: number;
}

/** What a search covers, crossing as a snake_case word. */
export type SearchScope = 'all_except_trash' | 'current_folder' | 'current_account';

/**
 * The mailbox list's search state, present exactly while a search runs: what
 * was asked for, what it covered, and what it found. The rows are the same
 * shape the list uses, because a search is a mode over the list, not a second
 * kind of list: whatever grouping the list is in, the results follow it.
 */
export interface SearchState {
  /** The query exactly as the user typed it, prefixes and all: the echo the
   * field shows, not the split the search ran with. */
  query: string;
  /** The scope the search ran under, echoed so a surface draws the filter the
   * results actually honour. */
  scope: SearchScope;
  /** The results, newest first, grouped exactly as the mailbox list groups. */
  rows: MessageRowView[];
  /** How far back the search looked, in whole months: the narrowest sync
   * depth among the accounts the scope covered. */
  depth_months: number;
}

/** The mailbox-list snapshot the panes render after every pull. */
export interface MailboxListSnapshot {
  /** Every account's sorted folder list, one entry per configured account. */
  account_folders: AccountFolderRow[];
  /** The message rows in display order; absent while no mail is listed. */
  rows?: MessageRowView[];
  /** The selected row's key; absent with nothing selected. */
  selection?: RowKey;
  /** How many rows the visible window holds; absent at rest. */
  visible_limit?: number;
  /** The mode the rows are grouped in; absent when flat, the default. */
  view_mode?: ViewMode;
  /** The search state; absent at rest, while no search is running. */
  search?: SearchState;
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
