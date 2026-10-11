// SPDX-License-Identifier: GPL-3.0-only
// The split inbox's derivation: the side a row lands on comes from the
// account's split map alone (a message key is unique only within its
// account), a thread row names no single message so no side claims it,
// and an unmapped row rides All only, never Actioned or FYI. The counts
// behind the toolbar's badges derive from the same one map.

import { beforeEach, describe, expect, it } from 'vitest';

import type { MessageRowView } from '@/services/surfaces';
import type { SplitEntry } from '@/services/ai';

import { sideOf, splitCounts, useSplitStore, visibleRows } from './splitStore';

/** A flat message row, overridable per test. */
function row(over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key: { message: { account: 'work', key: 'imap:v1:u1' } },
    account_id: 'work',
    sender_name: 'Ada Lovelace',
    sender_address: 'ada@example.test',
    subject: 'Quarterly report',
    sent_at: '2026-10-09T08:30:00Z',
    unread: true,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

const actioned = row({
  key: { message: { account: 'work', key: 'imap:v1:u2' } },
  subject: 'Needs your sign off',
});

const fyi = row({
  key: { message: { account: 'work', key: 'imap:v1:u3' } },
  subject: 'Your weekly digest',
});

const unmapped = row();

const thread = row({
  key: { thread: { account: 'work', thread_id: 'th-7' } },
  subject: 'Re: Compiler design',
  thread_count: 3,
});

/** The map over the four rows above: the unmapped and thread rows absent. */
const workEntries: SplitEntry[] = [
  { key: 'imap:v1:u2', side: 'actioned' },
  { key: 'imap:v1:u3', side: 'fyi' },
];

/** Applies one account's map to the store. */
function applyWork(entries: SplitEntry[]): void {
  useSplitStore.getState().applyMap('work', entries);
}

beforeEach(() => {
  useSplitStore.setState({ entries: {}, filter: 'all' });
});

describe('the side derivation', () => {
  it('answers the mapped side of a message row', () => {
    applyWork(workEntries);

    const entries = useSplitStore.getState().entries;
    expect(sideOf(actioned, entries)).toBe('actioned');
    expect(sideOf(fyi, entries)).toBe('fyi');
  });

  it('answers null for an unmapped row, a thread row, and a foreign account', () => {
    applyWork(workEntries);

    const entries = useSplitStore.getState().entries;
    expect(sideOf(unmapped, entries)).toBeNull();
    expect(sideOf(thread, entries)).toBeNull();
    expect(
      sideOf(row({ key: { message: { account: 'home', key: 'imap:v1:u2' } }, account_id: 'home' }), entries),
    ).toBeNull();
  });

  it('resolves the same provider key per its own account', () => {
    applyWork(workEntries);
    useSplitStore.getState().applyMap('home', [{ key: 'imap:v1:u2', side: 'fyi' }]);

    const entries = useSplitStore.getState().entries;
    expect(sideOf(actioned, entries)).toBe('actioned');
    expect(
      sideOf(row({ key: { message: { account: 'home', key: 'imap:v1:u2' } }, account_id: 'home' }), entries),
    ).toBe('fyi');
  });
});

describe('the visible rows', () => {
  it('shows every row under All', () => {
    applyWork(workEntries);

    const visible = visibleRows([actioned, fyi, unmapped, thread], 'all', useSplitStore.getState().entries);
    expect(visible).toHaveLength(4);
  });

  it('shows only the mapped rows under Actioned and FYI, unmapped and thread rows never', () => {
    applyWork(workEntries);
    const entries = useSplitStore.getState().entries;

    expect(visibleRows([actioned, fyi, unmapped, thread], 'actioned', entries).map((r) => r.subject)).toEqual([
      'Needs your sign off',
    ]);
    expect(visibleRows([actioned, fyi, unmapped, thread], 'fyi', entries).map((r) => r.subject)).toEqual([
      'Your weekly digest',
    ]);
  });

  it('keeps the row order the snapshot sent', () => {
    applyWork(workEntries);

    const visible = visibleRows([fyi, unmapped, actioned], 'actioned', useSplitStore.getState().entries);
    expect(visible.map((r) => r.subject)).toEqual(['Needs your sign off']);
  });
});

describe('the counts', () => {
  it('counts every side against the whole snapshot', () => {
    applyWork(workEntries);

    expect(splitCounts([actioned, fyi, unmapped, thread], useSplitStore.getState().entries)).toEqual({
      all: 4,
      actioned: 1,
      fyi: 1,
    });
  });

  it('counts an empty map as all rows on no side', () => {
    expect(splitCounts([actioned, fyi], useSplitStore.getState().entries)).toEqual({
      all: 2,
      actioned: 0,
      fyi: 0,
    });
  });
});

describe('the store', () => {
  it('replaces one account map on the next apply', () => {
    applyWork(workEntries);
    applyWork([{ key: 'imap:v1:u2', side: 'fyi' }]);

    const entries = useSplitStore.getState().entries;
    expect(entries['work']?.get('imap:v1:u2')).toBe('fyi');
    expect(entries['work']?.get('imap:v1:u3')).toBeUndefined();
  });

  it('holds the chosen filter, All until a toggle names one', () => {
    expect(useSplitStore.getState().filter).toBe('all');
    useSplitStore.getState().setFilter('fyi');
    expect(useSplitStore.getState().filter).toBe('fyi');
    useSplitStore.getState().setFilter('all');
    expect(useSplitStore.getState().filter).toBe('all');
  });
});
