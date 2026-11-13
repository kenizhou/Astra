// SPDX-License-Identifier: GPL-3.0-only
// The row menu's pin entry: the Pin/Unpin label flip on the row's pinned
// state, the position after Open in New Window, the toggle writing the
// local pin set (no intent), and the batch landing one pin state. The base
// menu behaviour lives in rowMenu.test.ts.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));
const window = vi.hoisted(() => ({ openMessageWindow: vi.fn() }));
const draftOpen = vi.hoisted(() => ({
  openDraftRow: vi.fn(),
  openDraftRowWindow: vi.fn(),
}));
const sweep = vi.hoisted(() => ({ open: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

vi.mock('@/features/reader/readerWindows', () => ({
  openMessageWindow: window.openMessageWindow,
}));

vi.mock('@/features/composer/draftOpen', () => ({
  openDraftRow: draftOpen.openDraftRow,
  openDraftRowWindow: draftOpen.openDraftRowWindow,
}));

vi.mock('@/features/sweep/sweepStore', () => ({
  useSweepStore: { getState: () => ({ open: sweep.open }) },
}));

import { buildRowMenu } from './rowMenu';
import { usePinnedRowsStore } from '@/features/list-pin/pinnedRowsStore';
import { rowKeyId } from '@/components/message-list/rowIdentity';
import type { MessageRowView, RowKey } from '@/services/surfaces';

function row(key: RowKey, over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key,
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'Quarterly report',
    sent_at: '2026-10-09T08:30:00Z',
    unread: false,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

const m1 = row({ message: { account: 'work', key: 'k1' } });
const m2 = row({ message: { account: 'work', key: 'k2' } });
const th = row({ thread: { account: 'work', thread_id: 'th-1' } });
const home = row({ message: { account: 'home', key: 'k1' } }, { account_id: 'home' });

/** The labels of the menu's items, separators as '|'. */
function labels(items: ReturnType<typeof buildRowMenu>): string[] {
  return items.map((item) => (item.separator ? '|' : (item.label ?? '?')));
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  localStorage.clear();
  usePinnedRowsStore.setState({ pinned: new Set() });
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe('the row menu pin entry', () => {
  it('offers Pin right after Open in New Window on an unpinned row', () => {
    const items = buildRowMenu([m1], 'flat');
    const positions = labels(items);
    expect(positions.indexOf('Pin')).toBe(positions.indexOf('Open in New Window') + 1);
    // The VIP entry stands beside Pin in the view-state group; the
    // separator closes the pair.
    expect(positions[positions.indexOf('Pin') + 1]).toBe('Add VIP');
    expect(positions[positions.indexOf('Pin') + 2]).toBe('|');

    const entry = items.find((item) => item.label === 'Pin')!;
    expect(entry.disabled).toBeUndefined();
    expect(entry.icon).toBeTruthy();
  });

  it('flips to Unpin on a pinned row', () => {
    usePinnedRowsStore.getState().toggle(rowKeyId(m1.key));
    const items = buildRowMenu([m1], 'flat');
    expect(items.find((item) => item.label === 'Unpin')).toBeTruthy();
    expect(items.find((item) => item.label === 'Pin')).toBeUndefined();
  });

  it('pins the row through the store, dispatching no intent', () => {
    const items = buildRowMenu([m1], 'flat');
    items.find((item) => item.label === 'Pin')!.onSelect!();

    expect(usePinnedRowsStore.getState().isPinned(rowKeyId(m1.key))).toBe(true);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('unpins a pinned row through the store', () => {
    usePinnedRowsStore.getState().toggle(rowKeyId(m1.key));
    const items = buildRowMenu([m1], 'flat');
    items.find((item) => item.label === 'Unpin')!.onSelect!();

    expect(usePinnedRowsStore.getState().isPinned(rowKeyId(m1.key))).toBe(false);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('pins a mixed batch whole, leaving the already pinned alone', () => {
    usePinnedRowsStore.getState().toggle(rowKeyId(m1.key));
    const items = buildRowMenu([m1, m2], 'flat');
    expect(items.find((item) => item.label === 'Pin')).toBeTruthy();

    items.find((item) => item.label === 'Pin')!.onSelect!();

    expect(usePinnedRowsStore.getState().isPinned(rowKeyId(m1.key))).toBe(true);
    expect(usePinnedRowsStore.getState().isPinned(rowKeyId(m2.key))).toBe(true);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('unpins an all-pinned batch', () => {
    usePinnedRowsStore.getState().toggle(rowKeyId(m1.key));
    usePinnedRowsStore.getState().toggle(rowKeyId(m2.key));
    const items = buildRowMenu([m1, m2], 'flat');
    expect(items.find((item) => item.label === 'Unpin')).toBeTruthy();

    items.find((item) => item.label === 'Unpin')!.onSelect!();

    expect(usePinnedRowsStore.getState().isPinned(rowKeyId(m1.key))).toBe(false);
    expect(usePinnedRowsStore.getState().isPinned(rowKeyId(m2.key))).toBe(false);
  });

  it('pins a thread row by its conversation identity', () => {
    const items = buildRowMenu([th], 'threaded');
    items.find((item) => item.label === 'Pin')!.onSelect!();

    expect(usePinnedRowsStore.getState().isPinned(rowKeyId(th.key))).toBe(true);
    expect(usePinnedRowsStore.getState().isPinned(rowKeyId(m1.key))).toBe(false);
  });

  it('scopes the label per account', () => {
    usePinnedRowsStore.getState().toggle(rowKeyId(home.key));
    // The work row shares the provider key but not the account: still Pin.
    const items = buildRowMenu([m1], 'flat');
    expect(items.find((item) => item.label === 'Pin')).toBeTruthy();
    expect(items.find((item) => item.label === 'Unpin')).toBeUndefined();
  });
});
