// SPDX-License-Identifier: GPL-3.0-only
// The folder pane's state: the last mailbox-list snapshot's account groups and
// the folder the user opened. Expansion state deliberately lives in the core,
// not here; the store only mirrors what each pull brought back, so the tree
// never disagrees with itself across surfaces or across a restart.

import { create } from 'zustand';

import type { FolderRef } from '@/services/ipc';
import type { AccountFolderRow, FolderRow, MailboxListSnapshot } from '@/services/surfaces';

interface FolderStoreState {
  /** Every account group with its sorted folder rows, in snapshot order. */
  accounts: AccountFolderRow[];
  /** The folder whose mail the list shows; null before the first selection. */
  selected: FolderRef | null;
  /** Replaces the tree from a fresh mailbox-list snapshot. */
  applySnapshot: (snapshot: MailboxListSnapshot) => void;
  /** Records a selection the surface just dispatched. */
  select: (folder: FolderRef) => void;
}

export const useFolderStore = create<FolderStoreState>((set) => ({
  accounts: [],
  selected: null,

  applySnapshot: (snapshot) => set({ accounts: snapshot.account_folders }),

  select: (folder) => set({ selected: folder }),
}));

/**
 * The rows of one account that are on screen: the snapshot is already ordered
 * depth-first, so a row shows when every ancestor the wire names is expanded.
 * The account's own header gate comes first: a collapsed account hides all.
 */
export function visibleFolders(account: AccountFolderRow): FolderRow[] {
  if (!account.expanded) {
    return [];
  }
  const byKey = new Map(account.folders.map((row) => [row.key, row]));
  return account.folders.filter((row) => {
    let parent = row.parent;
    while (parent !== undefined) {
      const ancestor = byKey.get(parent);
      if (ancestor === undefined || !ancestor.expanded) {
        return false;
      }
      parent = ancestor.parent;
    }
    return true;
  });
}
