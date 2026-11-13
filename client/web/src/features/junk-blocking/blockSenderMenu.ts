// SPDX-License-Identifier: GPL-3.0-only
// The row menu's Block sender entry, kept out of rowMenu.ts so that file
// stays under the line cap: a single row opens the block dialog over its
// sender, carrying the report capability the caller resolved (the spam
// verb's own visibility rule). A batch cannot name one sender, so the entry
// stands disabled with the reason. The block is a preference write the dialog
// dispatches; this entry only opens it.

import { JunkIcon } from '@/components/icons';
import type { ContextMenuItem } from '@/components/ui/ContextMenu';
import type { MessageRowView } from '@/services/surfaces';

import { useBlockSenderStore } from './blockSenderStore';

/** The Block sender entry over the targets, `canReport` the report leg's
 *  visibility the caller resolved from the account context. */
export function blockSenderItem(
  targets: MessageRowView[],
  canReport: boolean,
): ContextMenuItem {
  const one = targets.length === 1 ? targets[0]! : null;
  const reason = one === null ? 'Pick one row for this' : undefined;
  return {
    label: 'Block sender…',
    icon: JunkIcon,
    disabled: reason !== undefined ? true : undefined,
    title: reason,
    description: reason,
    onSelect: () => {
      if (one === null) {
        return;
      }
      useBlockSenderStore.getState().open({
        address: one.sender_address,
        row: one.key,
        canReport,
      });
    },
  };
}