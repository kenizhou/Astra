// SPDX-License-Identifier: GPL-3.0-only
// The Unified row's badge arithmetic (UI1 D4): the sum of per-mailbox unread
// counts across every account, junk and trash roles excluded, partial data
// summed without ceremony. Counts are per-mailbox, so no dedup is needed.

import type { AccountFolderRow } from '@/services/surfaces';

/** The unified unread total over the accounts the snapshot carries. */
export function unifiedUnread(accounts: AccountFolderRow[]): number {
  let total = 0;
  for (const account of accounts) {
    for (const folder of account.folders) {
      if (folder.role === 'junk' || folder.role === 'trash') {
        continue;
      }
      total += folder.unread;
    }
  }
  return total;
}
