// SPDX-License-Identifier: GPL-3.0-only
// The folder menu's items: sync and the inline create work today; the rest
// stand as placeholders, rename and delete carrying kylins' own reason on
// the special (role) folders they could never apply to.

import { FolderIcon, PlusIcon, RefreshIcon } from '@/components/icons';
import { placeholderItem, type ContextMenuItem } from '@/components/ui/ContextMenu';
import { dispatch, intents } from '@/services/ipc';
import type { FolderRow } from '@/services/surfaces';

/**
 * The menu one folder row opens. `onCreateSubfolder` asks the tree to open
 * its inline edit row beneath this folder.
 */
export function buildFolderMenu(
  folder: FolderRow,
  onCreateSubfolder: () => void,
): ContextMenuItem[] {
  const special = folder.role !== undefined;
  return [
    {
      label: 'Sync this folder',
      icon: RefreshIcon,
      onSelect: () => void dispatch(intents.refreshMail()),
    },
    {
      label: 'New Subfolder',
      icon: PlusIcon,
      onSelect: onCreateSubfolder,
    },
    { separator: true },
    placeholderItem('Mark All as Read', 'Marking a whole folder read arrives with a future slice'),
    special
      ? placeholderItem('Rename Folder', 'Special folders cannot be renamed')
      : placeholderItem('Rename Folder', 'Renaming a folder arrives with a future slice'),
    special
      ? { ...placeholderItem('Delete Folder', 'Special folders cannot be deleted'), danger: true, icon: FolderIcon }
      : {
          ...placeholderItem('Delete Folder', 'Deleting a folder arrives with a future slice'),
          danger: true,
          icon: FolderIcon,
        },
  ];
}
