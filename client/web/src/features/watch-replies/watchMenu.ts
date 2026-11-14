// SPDX-License-Identifier: GPL-3.0-only
// The row menu's Watch replies entry, kept out of rowMenu.ts so that file
// stays under the line cap (the blockSenderMenu pattern): a single row
// watches its conversation for a first reply, the label flipping to Unwatch
// when the conversation already stands watched. A batch cannot name one
// conversation, so the entry stands disabled with the reason. The watch is a
// host command over a local membership mark: no intent, no engine write, and
// the watch never moves mail.

import { EyeIcon } from '@/components/icons';
import type { ContextMenuItem } from '@/components/ui/ContextMenu';
import type { MessageRowView } from '@/services/surfaces';

import { useWatchStore } from './watchStore';

/** The Watch replies / Unwatch entry over the targets: a single row toggles
 *  its conversation's watch, a batch stands disabled with the reason. */
export function watchItem(targets: MessageRowView[]): ContextMenuItem {
  const one = targets.length === 1 ? targets[0]! : null;
  const reason = one === null ? 'Pick one row for this' : undefined;
  const watched = one !== null && useWatchStore.getState().isWatched(one);
  return {
    label: watched ? 'Unwatch' : 'Watch replies',
    icon: EyeIcon,
    disabled: reason !== undefined ? true : undefined,
    title: reason,
    description: reason,
    onSelect: () => {
      if (one === null) {
        return;
      }
      const store = useWatchStore.getState();
      if (store.isWatched(one)) {
        void store.unwatch(one);
      } else {
        void store.watch(one);
      }
    },
  };
}