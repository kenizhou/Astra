// SPDX-License-Identifier: GPL-3.0-only
// The folder menu's items: sync and the inline create as they stood, and the
// batch now live. Rename hands the tree its inline rename row; delete asks a
// confirm before the trash, or, inside the Trash, the permanent delete the
// honesty rule reserves for there; mark-all-read loops the listed rows. Every
// tree-edit verb hides while the account's provider cannot edit its tree.

import { FolderIcon, PlusIcon, RefreshIcon } from '@/components/icons';
import { type ContextMenuItem } from '@/components/ui/ContextMenu';
import { dispatch, intents } from '@/services/ipc';
import type { AccountFolderRow, FolderRow, MessageRowView } from '@/services/surfaces';

import { insideTrash } from './folderDrag';

/** What the tree hands the menu: its inline-row openers and the folder's
 *  listed rows, null while the list is showing some other folder. */
export interface FolderMenuHooks {
  /** Opens the tree's inline create row beneath this folder. */
  onCreateSubfolder: () => void;
  /** Opens the tree's inline rename row over this folder. */
  onRename: () => void;
  /** The folder's listed rows, when this folder is the one the list shows. */
  listedRows: MessageRowView[] | null;
}

/** The menu one folder row opens. */
export function buildFolderMenu(
  folder: FolderRow,
  account: AccountFolderRow,
  hooks: FolderMenuHooks,
): ContextMenuItem[] {
  const special = folder.role !== undefined;
  const capable = account.can_edit_folders === true;
  const items: ContextMenuItem[] = [
    {
      label: 'Sync this folder',
      icon: RefreshIcon,
      onSelect: () => void dispatch(intents.refreshMail()),
    },
  ];
  if (capable) {
    items.push({
      label: 'New Subfolder',
      icon: PlusIcon,
      onSelect: hooks.onCreateSubfolder,
    });
  }
  items.push({ separator: true });
  if (hooks.listedRows !== null) {
    items.push({
      label: 'Mark All as Read',
      onSelect: () => {
        for (const row of hooks.listedRows ?? []) {
          if (row.unread) {
            void dispatch(intents.markRead(row.key, true));
          }
        }
      },
    });
  }
  if (capable) {
    items.push(
      special
        ? {
            ...placeholder('Rename Folder', 'Special folders cannot be renamed'),
          }
        : { label: 'Rename Folder', onSelect: hooks.onRename },
      deleteItem(folder, account, special),
    );
  }
  return items;
}

/** One disabled item carrying its reason on both channels. */
function placeholder(label: string, reason: string): ContextMenuItem {
  return { label, disabled: true, title: reason, description: reason };
}

/** The delete item: the permanent verb inside the Trash, the recoverable one
 *  everywhere else, each behind its own confirm. A special folder outside the
 *  Trash never deletes, so the item stands disabled with its reason. */
function deleteItem(
  folder: FolderRow,
  account: AccountFolderRow,
  special: boolean,
): ContextMenuItem {
  const permanent = insideTrash(folder.key, account.folders);
  const untouchable = special && !permanent;
  const reason = 'Special folders cannot be deleted';
  return {
    label: 'Delete Folder',
    danger: true,
    icon: FolderIcon,
    disabled: untouchable,
    title: untouchable ? reason : undefined,
    description: untouchable ? reason : undefined,
    onSelect: () => {
      if (untouchable) {
        return;
      }
      const question = permanent
        ? `Permanently delete ${folder.name} and everything in it? This cannot be undone.`
        : `Delete ${folder.name}? Its mail moves to the Trash.`;
      if (!window.confirm(question)) {
        return;
      }
      void dispatch(
        permanent
          ? intents.deleteFolder(account.account_id, folder.key)
          : intents.trashFolder(account.account_id, folder.key),
      );
    },
  };
}
