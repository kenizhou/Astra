// SPDX-License-Identifier: GPL-3.0-only
// The category filter's predicate: a row carries the category when its id
// list names it, an absent list carries none, and the predicate ANDs onto the
// active tab (the two axes narrow together).
import { describe, expect, it } from 'vitest';

import { filteredRows, rowHasCategory } from './listFilter';
import type { MessageRowView } from '@/services/surfaces';

function row(key: string, over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key: { message: { account: 'work', key } },
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'One',
    sent_at: '2026-10-09T08:30:00Z',
    unread: false,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

describe('rowHasCategory', () => {
  it('is true when the row carries the id', () => {
    expect(rowHasCategory(row('k1', { categories: ['work'] }), 'work')).toBe(true);
  });

  it('is false when the row carries another id', () => {
    expect(rowHasCategory(row('k1', { categories: ['personal'] }), 'work')).toBe(false);
  });

  it('is false when the row carries no categories', () => {
    expect(rowHasCategory(row('k1'), 'work')).toBe(false);
  });
});

describe('the category axis ANDs onto the tab', () => {
  const rows = [
    row('k1', { unread: true, categories: ['work'] }),
    row('k2', { unread: true, categories: ['personal'] }),
    row('k3', { unread: false, categories: ['work'] }),
  ];

  it('narrows the unread tab to the unread rows carrying the category', () => {
    const unread = filteredRows(rows, 'unread');
    const narrowed = unread.filter((r) => rowHasCategory(r, 'work'));
    expect(narrowed.map((r) => r.key)).toEqual([{ message: { account: 'work', key: 'k1' } }]);
  });
});