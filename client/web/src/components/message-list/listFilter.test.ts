// SPDX-License-Identifier: GPL-3.0-only
// The list filter's predicates and counts: pure functions over the loaded
// rows, so the toolbar's badges and the list's visible set never disagree.

import { describe, expect, it } from 'vitest';

import { filteredRows, filterCounts, type ListFilter } from './listFilter';
import type { MessageRowView } from '@/services/surfaces';

function row(overrides: Partial<MessageRowView>): MessageRowView {
  return {
    key: { message: { account: 'work', key: 'k1' } },
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'Quarterly report',
    sent_at: '2026-10-09T08:30:00Z',
    unread: false,
    flagged: false,
    has_attachments: false,
    ...overrides,
  };
}

const rows: MessageRowView[] = [
  row({ key: { message: { account: 'work', key: 'k1' } }, unread: true }),
  row({ key: { message: { account: 'work', key: 'k2' } }, flagged: true }),
  row({ key: { message: { account: 'work', key: 'k3' } }, has_attachments: true }),
  row({ key: { message: { account: 'work', key: 'k4' } } }),
  row({
    key: { thread: { account: 'work', thread_id: 'th-1' } },
    unread: true,
    flagged: true,
    has_attachments: true,
    thread_count: 3,
  }),
];

describe('filteredRows', () => {
  it('all returns every row in the snapshot order', () => {
    expect(filteredRows(rows, 'all')).toHaveLength(5);
  });

  it('unread keeps the unread rows, thread rows included', () => {
    const kept = filteredRows(rows, 'unread');
    expect(kept).toHaveLength(2);
    expect(kept.every((r) => r.unread)).toBe(true);
  });

  it('flagged keeps the flagged rows', () => {
    const kept = filteredRows(rows, 'flagged');
    expect(kept).toHaveLength(2);
    expect(kept.every((r) => r.flagged)).toBe(true);
  });

  it('attachments keeps the rows carrying attachments', () => {
    const kept = filteredRows(rows, 'attachments');
    expect(kept).toHaveLength(2);
    expect(kept.every((r) => r.has_attachments)).toBe(true);
  });
});

describe('filterCounts', () => {
  it('counts every bucket over the same rows', () => {
    expect(filterCounts(rows)).toEqual({ all: 5, unread: 2, flagged: 2, attachments: 2, vip: 0 });
  });

  it('an empty list counts zero everywhere', () => {
    expect(filterCounts([])).toEqual({ all: 0, unread: 0, flagged: 0, attachments: 0, vip: 0 });
  });
});

describe('the ListFilter type', () => {
  it('names the five tabs', () => {
    const tabs: ListFilter[] = ['all', 'unread', 'flagged', 'attachments', 'vip'];
    expect(tabs).toHaveLength(5);
  });
});

describe('the vip tab', () => {
  const vipRows: MessageRowView[] = [
    row({ key: { message: { account: 'work', key: 'v1' } } }),
    row({ key: { message: { account: 'home', key: 'v2' } }, account_id: 'home' }),
    row({ key: { message: { account: 'work', key: 'v3' } }, sender_address: 'bob@example.test' }),
    row({
      key: { thread: { account: 'work', thread_id: 'th-v' } },
      thread_count: 2,
    }),
    row({ key: { message: { account: 'work', key: 'v4' } }, sender_address: 'ADA@Example.test ' }),
  ];
  const vip = new Set(['ada@example.test']);

  it('answers the VIP senders\' rows, cross-account and thread rows included', () => {
    const kept = filteredRows(vipRows, 'vip', vip);
    expect(kept).toHaveLength(4);
    expect(kept.some((r) => r.sender_address === 'bob@example.test')).toBe(false);
    expect(kept.some((r) => r.account_id === 'home')).toBe(true);
    expect(kept.some((r) => 'thread' in r.key)).toBe(true);
  });

  it('normalises the row address against the set', () => {
    const kept = filteredRows(vipRows, 'vip', vip);
    expect(kept.some((r) => r.sender_address === 'ADA@Example.test ')).toBe(true);
  });

  it('answers nothing for an empty VIP set', () => {
    expect(filteredRows(vipRows, 'vip')).toHaveLength(0);
    expect(filteredRows(vipRows, 'vip', new Set())).toHaveLength(0);
  });

  it('counts the VIP rows over the unfiltered set', () => {
    expect(filterCounts(vipRows, vip).vip).toBe(4);
    expect(filterCounts(vipRows).vip).toBe(0);
    expect(filterCounts([], vip).vip).toBe(0);
  });

  it('leaves the other tabs untouched by the VIP set', () => {
    expect(filteredRows(vipRows, 'all', vip)).toHaveLength(5);
    expect(filteredRows(vipRows, 'unread', vip)).toHaveLength(0);
  });
});
