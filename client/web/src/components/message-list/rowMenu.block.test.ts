// SPDX-License-Identifier: GPL-3.0-only
// The row menu's Block sender entry: its position beside VIP, the single-row
// gate, the dialog it opens through the block store, the report capability
// it carries, and the reference order (blocked beats VIP): a VIP sender is
// offered the block and blocking them opens the dialog, VIP granting no
// exemption. The base menu behaviour lives in rowMenu.test.ts.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));
const blockStore = vi.hoisted(() => ({ open: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

vi.mock('@/features/junk-blocking/blockSenderStore', () => ({
  useBlockSenderStore: { getState: () => ({ open: blockStore.open }) },
}));

import { buildRowMenu, type RowAccountContext } from './rowMenu';
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
const m2 = row({ message: { account: 'work', key: 'k2' } });
const th = row({ thread: { account: 'work', thread_id: 'th-1' } });

/** A context whose provider can report and whose tree resolves a Junk role. */
const reportable: RowAccountContext = {
  account: 'work',
  currentKey: 'inbox',
  canReportMail: true,
  folders: [
    { key: 'inbox', name: 'Inbox', role: 'inbox', unread: 0, depth: 0, has_children: false, expanded: false },
    { key: 'junk', name: 'Junk', role: 'junk', unread: 0, depth: 0, has_children: false, expanded: false },
  ],
};

/** The labels of the menu's items, separators as '|'. */
function labels(items: ReturnType<typeof buildRowMenu>): string[] {
  return items.map((item) => (item.separator ? '|' : (item.label ?? '?')));
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  blockStore.open.mockReset();
  localStorage.clear();
  useVipSendersStore.setState({ vip: new Set() });
});

describe('the row menu Block sender entry', () => {
  it('offers Block sender right after the VIP entry', () => {
    const items = buildRowMenu([m1], 'flat');
    const positions = labels(items);
    const vipIndex = Math.max(positions.indexOf('Add VIP'), positions.indexOf('Remove VIP'));
    expect(positions.indexOf('Block sender…')).toBe(vipIndex + 1);
  });

  it('opens the block dialog over the row sender and key', () => {
    const items = buildRowMenu([m1], 'flat', reportable);
    items.find((item) => item.label === 'Block sender…')!.onSelect!();

    expect(blockStore.open).toHaveBeenCalledWith({
      address: 'ada@example.test',
      row: m1.key,
      canReport: true,
    });
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('carries canReport false where the provider cannot report', () => {
    const items = buildRowMenu([m1], 'flat', { ...reportable, canReportMail: false });
    items.find((item) => item.label === 'Block sender…')!.onSelect!();

    expect(blockStore.open).toHaveBeenCalledWith(
      expect.objectContaining({ canReport: false }),
    );
  });

  it('carries canReport false where no junk destination resolves', () => {
    const items = buildRowMenu([m1], 'flat', {
      ...reportable,
      folders: [
        { key: 'inbox', name: 'Inbox', role: 'inbox', unread: 0, depth: 0, has_children: false, expanded: false },
      ],
    });
    items.find((item) => item.label === 'Block sender…')!.onSelect!();

    expect(blockStore.open).toHaveBeenCalledWith(
      expect.objectContaining({ canReport: false }),
    );
  });

  it('disables a batch with its reason', () => {
    const items = buildRowMenu([m1, m2], 'flat');
    const entry = items.find((item) => item.label === 'Block sender…')!;
    expect(entry.disabled).toBe(true);
    expect(entry.title).toBe('Pick one row for this');
    entry.onSelect!();
    expect(blockStore.open).not.toHaveBeenCalled();
  });

  it('opens over a conversation row by its sender', () => {
    const items = buildRowMenu([th], 'threaded', reportable);
    items.find((item) => item.label === 'Block sender…')!.onSelect!();

    expect(blockStore.open).toHaveBeenCalledWith({
      address: 'ada@example.test',
      row: th.key,
      canReport: true,
    });
  });

  it('blocked beats VIP: a VIP sender is still offered the block', () => {
    // The reference order: VIP grants no exemption from blocking. The mark
    // is the web's own; the block entry stands and opens the dialog over the
    // VIP sender exactly as over any other.
    useVipSendersStore.getState().add('ada@example.test');
    const items = buildRowMenu([m1], 'flat', reportable);
    expect(items.find((item) => item.label === 'Remove VIP')).toBeTruthy();
    const entry = items.find((item) => item.label === 'Block sender…')!;
    expect(entry.disabled).toBeUndefined();

    entry.onSelect!();
    expect(blockStore.open).toHaveBeenCalledWith(
      expect.objectContaining({ address: 'ada@example.test' }),
    );
  });
});