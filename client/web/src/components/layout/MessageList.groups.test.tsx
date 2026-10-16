// SPDX-License-Identifier: GPL-3.0-only
// The message list pane's grouping and view swaps: the date group headers
// the folder listing renders under (one per sent-at day, the weekday within
// the last week, Earlier for an unknown stamp), the density the rows and
// the virtualiser's estimate follow, and the outbox queue view the folder
// tree's Outbox row leads. The flat listing and the split filter are pinned
// in MessageList.test.tsx, the search results mode in
// MessageList.search.test.tsx.
import { act, cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  mailboxList: vi.fn(),
  syncProgress: vi.fn(),
  addAccount: vi.fn(),
  imapTestConnection: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

// KY's pattern: jsdom reports no viewport, so the real windowed virtualizer
// would render nothing. The mock returns every item, which pins that rows
// flow from the snapshot through the virtualizer's items into the tree, and
// captures the options so the density estimate is assertable.
const virtual = vi.hoisted(() => ({
  lastOpts: null as null | { count: number; estimateSize: (index: number) => number },
}));

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (opts: { count: number; estimateSize: (index: number) => number }) => {
    virtual.lastOpts = opts;
    return {
      getVirtualItems: () =>
        Array.from({ length: opts.count }, (_, i) => ({ key: String(i), index: i, start: i * 56 })),
      getTotalSize: () => opts.count * 56,
      measureElement: () => undefined,
      scrollToIndex: () => undefined,
    };
  },
}));

import { MessageList } from './MessageList';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { useSendingStore } from '@/stores/sendingStore';
import { useSplitStore } from '@/stores/splitStore';
import type { MailboxListSnapshot, MessageRowView } from '@/services/surfaces';
import fixtureJson from '@/services/__fixtures__/mailbox_list.json';

/** A flat row, overridable per test. */
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

const digest = row({
  key: { message: { account: 'work', key: 'imap:v1:u2' } },
  sender_name: '',
  sender_address: 'notifications@example.test',
  subject: 'Your weekly digest',
  sent_at: '2026-10-08T16:05:00Z',
  unread: false,
});

/** Applies one list view to the store: the rows, the window, and the mode. */
function applyList(snapshot: Partial<MailboxListSnapshot>): void {
  useListStore.getState().applySnapshot({ account_folders: [], ...snapshot });
}

/** The listbox option showing `text`. */
function optionWith(text: string): HTMLElement {
  const option = screen.getByText(text).closest('[role="option"]');
  expect(option).not.toBeNull();
  return option as HTMLElement;
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-10-09T12:00:00Z'));
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useFolderStore.setState({
    accounts: (fixtureJson as MailboxListSnapshot).account_folders,
    selected: { account: 'work', key: 'INBOX' },
    outboxAccount: null,
  });
  useListStore.setState({
    rows: [],
    viewMode: 'flat',
    visibleLimit: 0,
    selection: null,
    selectedKey: null,
    search: null,
    density: 'normal',
  });
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useSplitStore.setState({ entries: {} });
  useSendingStore.getState().applySnapshot({ rows: [] });
  virtual.lastOpts = null;
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('the date group headers', () => {
  it('groups rows on the same day under one header', () => {
    applyList({
      rows: [
        row(),
        row({
          key: { message: { account: 'work', key: 'imap:v1:u9' } },
          subject: 'Standup notes',
        }),
      ],
      visible_limit: 50,
    });

    render(<MessageList />);

    expect(screen.getAllByText('Today')).toHaveLength(1);
    expect(screen.getByText('Quarterly report')).toBeTruthy();
    expect(screen.getByText('Standup notes')).toBeTruthy();
  });

  it('gives rows across days distinct headers', () => {
    applyList({
      rows: [
        row(),
        digest,
        row({
          key: { message: { account: 'work', key: 'imap:v1:u10' } },
          subject: 'Monday sync',
          sent_at: '2026-10-05T09:00:00Z',
        }),
        row({
          key: { message: { account: 'work', key: 'imap:v1:u11' } },
          subject: 'September wrap',
          sent_at: '2026-09-30T09:00:00Z',
        }),
      ],
      visible_limit: 50,
    });

    render(<MessageList />);

    // Today and yesterday by name, the weekday within the last week, and
    // the bare date beyond it, newest first. The date header spells what
    // the row's own stamp spells, so it answers by its header role.
    expect(screen.getByText('Today')).toBeTruthy();
    expect(screen.getByText('Yesterday')).toBeTruthy();
    expect(screen.getByText('Monday')).toBeTruthy();
    expect(
      screen.getAllByText('30 Sep').some((el) => el.getAttribute('role') === 'presentation'),
    ).toBe(true);
    expect(screen.getAllByRole('option')).toHaveLength(4);
  });

  it('groups a row without a stamp under Earlier', () => {
    applyList({ rows: [row({ sent_at: '' })], visible_limit: 50 });

    render(<MessageList />);

    expect(screen.getByText('Earlier')).toBeTruthy();
  });
});

describe('the density', () => {
  it('draws rows at the normal height by default', () => {
    applyList({ rows: [row()], visible_limit: 50 });

    render(<MessageList />);

    expect(optionWith('Quarterly report').className).toContain('min-h-[56px]');
    // Item 0 is the Today header, item 1 the row.
    expect(virtual.lastOpts?.estimateSize(0)).toBe(30);
    expect(virtual.lastOpts?.estimateSize(1)).toBe(56);
  });

  it('re-renders rows at the new estimate when the density changes', () => {
    applyList({ rows: [row()], visible_limit: 50 });
    render(<MessageList />);

    act(() => {
      useListStore.getState().setDensity('compact');
    });
    expect(optionWith('Quarterly report').className).toContain('min-h-[44px]');
    expect(virtual.lastOpts?.estimateSize(1)).toBe(44);

    act(() => {
      useListStore.getState().setDensity('comfortable');
    });
    expect(optionWith('Quarterly report').className).toContain('min-h-[72px]');
    expect(virtual.lastOpts?.estimateSize(1)).toBe(72);
  });
});

describe('the outbox queue view', () => {
  it('swaps the folder rows for the account queue while the outbox leads', () => {
    applyList({ rows: [row()], visible_limit: 50 });
    useSendingStore.getState().applySnapshot({
      rows: [
        {
          queued: { account: 'work', op: 3 },
          state: 'queued',
          subject: 'Quarterly review',
          arrived_at: '2026-10-09T08:30:00Z',
        },
        { queued: { account: 'home', op: 1 }, state: 'queued', subject: 'Off shift', arrived_at: '' },
      ],
    });
    useFolderStore.getState().selectOutbox('work');

    render(<MessageList />);

    const pane = screen.getByRole('region', { name: 'Message list' });
    // The toolbar names the queue.
    expect(within(pane).getByText('Outbox')).toBeTruthy();
    const queue = within(pane).getByRole('list', { name: 'Outbox' });
    expect(within(queue).getByText('Quarterly review')).toBeTruthy();
    // The other account's send and the folder rows stay off.
    expect(within(queue).queryByText('Off shift')).toBeNull();
    expect(within(pane).queryByRole('option')).toBeNull();
    expect(within(pane).queryByText('Quarterly report')).toBeNull();
  });

  it('offers an empty state when the account owes no sends', () => {
    useFolderStore.getState().selectOutbox('work');

    render(<MessageList />);

    expect(screen.getByText('No sends waiting')).toBeTruthy();
  });

  it('returns to the folder view when a folder leads again', () => {
    applyList({ rows: [row()], visible_limit: 50 });
    useFolderStore.getState().selectOutbox('work');
    render(<MessageList />);
    expect(screen.queryByRole('option')).toBeNull();

    act(() => {
      useFolderStore.getState().select({ account: 'work', key: 'INBOX' });
    });

    expect(screen.getByText('Quarterly report')).toBeTruthy();
    expect(screen.queryByRole('list', { name: 'Outbox' })).toBeNull();
  });
});
