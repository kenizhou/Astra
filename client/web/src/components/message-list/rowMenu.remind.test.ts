// SPDX-License-Identifier: GPL-3.0-only
// The row menu's Remind me entry: the position after Snooze, the picker
// opening over the single message (no intent), and the disabled reasons on a
// batch and a conversation. The base menu behaviour lives in rowMenu.test.ts.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));
const window = vi.hoisted(() => ({ openMessageWindow: vi.fn() }));
const draftOpen = vi.hoisted(() => ({
  openDraftRow: vi.fn(),
  openDraftRowWindow: vi.fn(),
}));
const sweep = vi.hoisted(() => ({ open: vi.fn() }));
const snooze = vi.hoisted(() => ({ open: vi.fn() }));
const remind = vi.hoisted(() => ({ open: vi.fn() }));

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

vi.mock('@/features/snooze/snoozeStore', () => ({
  useSnoozeStore: { getState: () => ({ open: snooze.open }) },
}));

vi.mock('@/features/reminders/reminderStore', () => ({
  useReminderStore: { getState: () => ({ open: remind.open }) },
}));

import { buildRowMenu, type RowAccountContext } from './rowMenu';
import type { FolderRow, MessageRowView, RowKey } from '@/services/surfaces';

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

const accountCtx = {
  account: 'work',
  currentKey: 'INBOX',
  canReportMail: true,
  folders: [
    { key: 'Archive', name: 'Archive', unread: 0, depth: 0, has_children: false, expanded: false },
  ] as FolderRow[],
} satisfies RowAccountContext;

/** The labels of the menu's items, separators as '|'. */
function labels(items: ReturnType<typeof buildRowMenu>): string[] {
  return items.map((item) => (item.separator ? '|' : (item.label ?? '?')));
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  remind.open.mockReset();
});

describe('the row menu remind entry', () => {
  it('stands right after the snooze entry', () => {
    const positions = labels(buildRowMenu([m1], 'flat', accountCtx));
    expect(positions.indexOf('Remind me…')).toBe(positions.indexOf('Snooze…') + 1);
  });

  it('opens the reminder picker over the single message', () => {
    const items = buildRowMenu([m1], 'flat', accountCtx);
    const entry = items.find((item) => item.label === 'Remind me…')!;

    expect(entry.disabled).toBeUndefined();
    entry.onSelect!();

    expect(remind.open).toHaveBeenCalledTimes(1);
    expect(remind.open).toHaveBeenCalledWith({ account: 'work', key: 'k1' });
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('disables the remind entry on a batch and on a conversation, with the reason', () => {
    const batch = buildRowMenu([m1, m2], 'flat', accountCtx).find(
      (item) => item.label === 'Remind me…',
    )!;
    expect(batch.disabled).toBe(true);
    expect(batch.description).toContain('Pick one message');
    batch.onSelect?.();
    expect(remind.open).not.toHaveBeenCalled();

    const thread = buildRowMenu([th], 'flat', accountCtx).find(
      (item) => item.label === 'Remind me…',
    )!;
    expect(thread.disabled).toBe(true);
    expect(thread.description).toContain('conversation');
    thread.onSelect?.();
    expect(remind.open).not.toHaveBeenCalled();
  });
});
