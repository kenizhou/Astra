// SPDX-License-Identifier: GPL-3.0-only
// The Unified row's badge arithmetic (UI1 D4): the sum of per-mailbox unread
// counts across every account, junk and trash roles excluded, partial data
// summed without ceremony.
import { describe, expect, it } from 'vitest';

import { unifiedUnread } from './unifiedSum';
import type { AccountFolderRow } from '@/services/surfaces';

function account(id: string, unreadByFolder: Array<[string, number, string?]>): AccountFolderRow {
  return {
    account_id: id,
    display_name: id,
    address: `${id}@example.test`,
    expanded: true,
    folders: unreadByFolder.map(([key, unread, role]) => ({
      key,
      name: key,
      unread,
      depth: 0,
      has_children: false,
      expanded: false,
      ...(role !== undefined ? { role: role as 'junk' | 'trash' } : {}),
    })),
  };
}

describe('unifiedUnread', () => {
  it('sums the unread counts across accounts', () => {
    const accounts = [account('work', [['INBOX', 3], ['Clients/Acme', 2]]), account('home', [['INBOX', 7]])];
    expect(unifiedUnread(accounts)).toBe(12);
  });

  it('excludes the junk and trash roles', () => {
    const accounts = [account('work', [['INBOX', 3], ['Junk Mail', 9, 'junk'], ['Deleted Items', 4, 'trash']])];
    expect(unifiedUnread(accounts)).toBe(3);
  });

  it('sums the available accounts when one carries nothing', () => {
    const accounts = [account('work', [['INBOX', 3]]), account('empty', [])];
    expect(unifiedUnread(accounts)).toBe(3);
  });

  it('is zero with no accounts at all', () => {
    expect(unifiedUnread([])).toBe(0);
  });
});
