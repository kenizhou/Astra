// SPDX-License-Identifier: GPL-3.0-only
// The unified inbox row, live: clicking selects the unified scope (the
// select-account-null family, persisted by the core across launches), the
// unread badge keeps counting, and the row renders only while the pane holds
// at least two accounts.

import { InboxIcon } from '@/components/icons';
import { dispatch, intents } from '@/services/ipc';
import { unifiedUnread } from './unifiedSum';
import { useFolderStore } from '@/stores/folderStore';
import type { AccountFolderRow } from '@/services/surfaces';

/** The row, rendered only while the pane holds at least two accounts. */
export function UnifiedInboxRow({ accounts }: { accounts: AccountFolderRow[] }) {
  const selected =
    useFolderStore((s) => s.selected === null && s.outboxAccount === null && s.unified) &&
    accounts.length >= 2;
  if (accounts.length < 2) {
    return null;
  }
  const unread = unifiedUnread(accounts);
  const onSelect = () => {
    useFolderStore.getState().selectUnified();
    void dispatch(intents.selectAccount(null));
  };
  return (
    <div
      role="treeitem"
      aria-selected={selected}
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect();
        }
      }}
      onClick={onSelect}
      className={`flex min-h-[var(--row-h)] cursor-pointer items-center gap-2 rounded-lg px-2 text-[13px] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
        selected
          ? 'bg-selected font-medium text-[var(--selected-text)]'
          : 'text-foreground hover:bg-hover'
      }`}
    >
      <span aria-hidden="true" className="flex w-4 shrink-0 items-center justify-center">
        <InboxIcon size={15} />
      </span>
      <span className="min-w-0 flex-1 truncate text-left">Unified inbox</span>
      {unread > 0 && (
        <span className="shrink-0 rounded-full bg-primary px-1.5 py-0.5 text-[11px] font-medium tabular-nums leading-none text-primary-foreground">
          {unread}
        </span>
      )}
    </div>
  );
}
