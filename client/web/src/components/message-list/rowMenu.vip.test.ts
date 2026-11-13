// SPDX-License-Identifier: GPL-3.0-only
// The row menu's VIP entry: the Add VIP/Remove VIP label flip on the
// sender's VIP state, the position after Pin, the toggle writing the local
// VIP set (no intent), the batch landing one VIP state, and the mark
// standing across accounts because the set keys by address alone. The base
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
import { useVipSendersStore } from '@/features/vip/vipSendersStore';
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
const m2 = row({ message: { account: 'work', key: 'k2' } }, { sender_address: 'bob@example.test' });
const th = row({ thread: { account: 'work', thread_id: 'th-1' } });
const home = row({ message: { account: 'home', key: 'k1' } }, { account_id: 'home' });
const blank = row({ message: { account: 'work', key: 'k3' } }, { sender_address: '' });

/** The labels of the menu's items, separators as '|'. */
function labels(items: ReturnType<typeof buildRowMenu>): string[] {
  return items.map((item) => (item.separator ? '|' : (item.label ?? '?')));
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  localStorage.clear();
  useVipSendersStore.setState({ vip: new Set() });
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe('the row menu VIP entry', () => {
  it('offers Add VIP right after Pin on an unmarked sender', () => {
    const items = buildRowMenu([m1], 'flat');
    const positions = labels(items);
    expect(positions.indexOf('Add VIP')).toBe(positions.indexOf('Pin') + 1);

    const entry = items.find((item) => item.label === 'Add VIP')!;
    expect(entry.disabled).toBeUndefined();
    expect(entry.icon).toBeTruthy();
  });

  it('flips to Remove VIP when the sender stands VIP', () => {
    useVipSendersStore.getState().add('ada@example.test');
    const items = buildRowMenu([m1], 'flat');
    expect(items.find((item) => item.label === 'Remove VIP')).toBeTruthy();
    expect(items.find((item) => item.label === 'Add VIP')).toBeUndefined();
  });

  it('marks the sender through the store, dispatching no intent', () => {
    const items = buildRowMenu([m1], 'flat');
    items.find((item) => item.label === 'Add VIP')!.onSelect!();

    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(true);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('clears the mark through the store', () => {
    useVipSendersStore.getState().add('ada@example.test');
    const items = buildRowMenu([m1], 'flat');
    items.find((item) => item.label === 'Remove VIP')!.onSelect!();

    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(false);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('marks a mixed batch whole, leaving the already VIP alone', () => {
    useVipSendersStore.getState().add('ada@example.test');
    const items = buildRowMenu([m1, m2], 'flat');
    expect(items.find((item) => item.label === 'Add VIP')).toBeTruthy();

    items.find((item) => item.label === 'Add VIP')!.onSelect!();

    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(true);
    expect(useVipSendersStore.getState().has('bob@example.test')).toBe(true);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('clears an all-VIP batch', () => {
    useVipSendersStore.getState().add('ada@example.test');
    useVipSendersStore.getState().add('bob@example.test');
    const items = buildRowMenu([m1, m2], 'flat');
    expect(items.find((item) => item.label === 'Remove VIP')).toBeTruthy();

    items.find((item) => item.label === 'Remove VIP')!.onSelect!();

    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(false);
    expect(useVipSendersStore.getState().has('bob@example.test')).toBe(false);
  });

  it('stands across accounts: the same address elsewhere reads VIP', () => {
    useVipSendersStore.getState().add('ada@example.test');
    // The home row shares the sender address though not the account: the
    // mark is the sender's, so the label flips there too.
    const items = buildRowMenu([home], 'flat');
    expect(items.find((item) => item.label === 'Remove VIP')).toBeTruthy();
    expect(items.find((item) => item.label === 'Add VIP')).toBeUndefined();
  });

  it('marks a thread row by its sender address', () => {
    const items = buildRowMenu([th], 'threaded');
    items.find((item) => item.label === 'Add VIP')!.onSelect!();

    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(true);
  });

  it('normalises the row address against the set', () => {
    useVipSendersStore.getState().add('ada@example.test');
    const loud = row({ message: { account: 'work', key: 'k4' } }, { sender_address: 'ADA@Example.test' });
    const items = buildRowMenu([loud], 'flat');
    expect(items.find((item) => item.label === 'Remove VIP')).toBeTruthy();
  });

  it('leaves a blank sender alone in a batch', () => {
    const items = buildRowMenu([m1, blank], 'flat');
    items.find((item) => item.label === 'Add VIP')!.onSelect!();

    expect(useVipSendersStore.getState().has('ada@example.test')).toBe(true);
    expect(useVipSendersStore.getState().vip.size).toBe(1);
  });
});