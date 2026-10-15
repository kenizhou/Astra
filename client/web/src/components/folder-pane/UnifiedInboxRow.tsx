// SPDX-License-Identifier: GPL-3.0-only
// The Unified inbox row (UI1 D4): the multi-account visual language ahead of
// SP2p's behaviour. The row is inert in this slice: aria-disabled with normal
// pointer events (a genuinely disabled control would swallow the tooltip),
// focusable, and its tooltip answers both hover and keyboard focus. The badge
// is live: the sum of the snapshot's unread counts, minus junk and trash.

import { InboxIcon } from '@/components/icons';
import { unifiedUnread } from './unifiedSum';
import type { AccountFolderRow } from '@/services/surfaces';

/** The row, rendered only while the pane holds at least two accounts. */
export function UnifiedInboxRow({ accounts }: { accounts: AccountFolderRow[] }) {
  if (accounts.length < 2) {
    return null;
  }
  const unread = unifiedUnread(accounts);
  return (
    <div
      role="treeitem"
      aria-disabled="true"
      aria-describedby="unified-inbox-note"
      tabIndex={0}
      title="Unified inbox arrives with SP2p"
      className="flex min-h-[var(--row-h)] items-center gap-2 rounded-lg px-2 text-[13px] text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
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
      <span id="unified-inbox-note" className="sr-only">
        Unified inbox arrives with SP2p; the row is a preview and does not open yet.
      </span>
    </div>
  );
}
