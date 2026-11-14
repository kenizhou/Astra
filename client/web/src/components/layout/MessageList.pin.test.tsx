// SPDX-License-Identifier: GPL-3.0-only
// The message list's pin overlay: pinned rows float to the top with the
// engine order kept within each partition, the persistent pin indicator,
// the live re-order on a mid-view toggle, and the overlay leaving the
// selection (click, Shift range, Select All), ShowMore, and the search
// results mode intact. The base list behaviour lives in MessageList.test.tsx.
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  mailboxList: vi.fn(),
  syncProgress: vi.fn(),
  addAccount: vi.fn(),
  imapTestConnection: vi.fn(),
}));

vi.mock('@/services/categories', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listCategories: vi.fn().mockResolvedValue([]),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

// KY's pattern: jsdom reports no viewport, so the real windowed virtualizer
// would render nothing. The mock returns every item, which pins that rows
// flow from the snapshot through the virtualizer's items into the tree.
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (opts: { count: number }) => ({
    getVirtualItems: () =>
      Array.from({ length: opts.count }, (_, i) => ({ key: String(i), index: i, start: i * 56 })),
    getTotalSize: () => opts.count * 56,
    measureElement: () => undefined,
    scrollToIndex: () => undefined,
  }),
}));

import { MessageList } from './MessageList';
import { usePinnedRowsStore } from '@/features/list-pin/pinnedRowsStore';
import { rowKeyId } from '@/components/message-list/rowIdentity';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
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

const newest = row();
const middle = row({
  key: { message: { account: 'work', key: 'imap:v1:u2' } },
  subject: 'Your weekly digest',
  sent_at: '2026-10-08T16:05:00Z',
  unread: false,
});
const oldest = row({
  key: { message: { account: 'work', key: 'imap:v1:u3' } },
  subject: 'Compiler design notes',
  sent_at: '2026-10-07T09:15:00Z',
  unread: false,
});
const fourth = row({
  key: { message: { account: 'work', key: 'imap:v1:u4' } },
  subject: 'Lunch on Friday',
  sent_at: '2026-10-06T12:00:00Z',
  unread: false,
});

/** The subjects the order assertions read, one per fixture row. */
const SUBJECTS = [newest, middle, oldest, fourth].map((r) => r.subject);

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

/** The rendered rows' subjects, in render order. */
function renderedOrder(): string[] {
  return within(screen.getByRole('listbox'))
    .getAllByRole('option')
    .map((option) => SUBJECTS.find((s) => option.textContent?.includes(s)) ?? '?');
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-10-09T12:00:00Z'));
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  localStorage.clear();
  usePinnedRowsStore.setState({ pinned: new Set() });
  useFolderStore.setState({
    accounts: (fixtureJson as MailboxListSnapshot).account_folders,
    selected: { account: 'work', key: 'INBOX' },
    outboxAccount: null,
    scope: { kind: 'folder', account: 'work', key: 'INBOX' },
    unified: false,
  });
  useListStore.setState({
    rows: [],
    viewMode: 'flat',
    visibleLimit: 0,
    selection: null,
    selectedKey: null,
    selectedKeys: [],
    anchorKey: null,
    search: null,
    density: 'normal',
  });
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useSplitStore.setState({ entries: {} });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  localStorage.clear();
});

describe('the list pin overlay', () => {
  it('floats the pinned rows first, engine order within each partition', () => {
    applyList({ rows: [newest, middle, oldest, fourth], visible_limit: 50 });
    usePinnedRowsStore.getState().toggle(rowKeyId(oldest.key));
    usePinnedRowsStore.getState().toggle(rowKeyId(middle.key));

    render(<MessageList />);

    // middle before oldest (engine order, though oldest is older), then the
    // unpinned rest in engine order.
    expect(renderedOrder()).toEqual([
      'Your weekly digest',
      'Compiler design notes',
      'Quarterly report',
      'Lunch on Friday',
    ]);
  });

  it('draws the persistent pin indicator on pinned rows only', () => {
    applyList({ rows: [newest, middle, oldest], visible_limit: 50 });
    usePinnedRowsStore.getState().toggle(rowKeyId(oldest.key));

    render(<MessageList />);

    // No hover, no selection: the indicator stands on its own.
    expect(within(optionWith('Compiler design notes')).getByTitle('Pinned')).toBeTruthy();
    expect(within(optionWith('Quarterly report')).queryByTitle('Pinned')).toBeNull();
    expect(within(optionWith('Your weekly digest')).queryByTitle('Pinned')).toBeNull();
  });

  it('re-orders live when a pin toggles mid-view', () => {
    applyList({ rows: [newest, middle, oldest], visible_limit: 50 });

    render(<MessageList />);
    expect(renderedOrder()).toEqual(['Quarterly report', 'Your weekly digest', 'Compiler design notes']);

    act(() => {
      usePinnedRowsStore.getState().toggle(rowKeyId(oldest.key));
    });
    expect(renderedOrder()).toEqual(['Compiler design notes', 'Quarterly report', 'Your weekly digest']);
    expect(within(optionWith('Compiler design notes')).getByTitle('Pinned')).toBeTruthy();

    act(() => {
      usePinnedRowsStore.getState().toggle(rowKeyId(oldest.key));
    });
    expect(renderedOrder()).toEqual(['Quarterly report', 'Your weekly digest', 'Compiler design notes']);
    expect(screen.queryByTitle('Pinned')).toBeNull();
  });

  it('keeps the selection and open flow on a floated row', () => {
    applyList({ rows: [newest, middle, oldest], visible_limit: 50 });
    usePinnedRowsStore.getState().toggle(rowKeyId(oldest.key));

    render(<MessageList />);
    fireEvent.click(screen.getByText('Compiler design notes'));

    expect(optionWith('Compiler design notes').getAttribute('aria-selected')).toBe('true');
    expect(optionWith('Quarterly report').getAttribute('aria-selected')).toBe('false');
    expect(ipc.dispatch).toHaveBeenCalledWith({
      open_message: { message: { account: 'work', key: 'imap:v1:u3' } },
    });
  });

  it('ranges over the rendered order with Shift', () => {
    applyList({ rows: [newest, middle, oldest, fourth], visible_limit: 50 });
    usePinnedRowsStore.getState().toggle(rowKeyId(oldest.key));

    render(<MessageList />);
    // Rendered order: oldest, newest, middle, fourth. The anchor is oldest;
    // the Shift range to middle takes the rendered run between them.
    fireEvent.click(screen.getByText('Compiler design notes'));
    fireEvent.click(screen.getByText('Your weekly digest'), { shiftKey: true });

    expect(useListStore.getState().selectedKeys).toEqual([oldest.key, newest.key, middle.key]);
  });

  it('Select All takes every loaded row over the partitioned view', () => {
    applyList({ rows: [newest, middle, oldest, fourth], visible_limit: 50 });
    usePinnedRowsStore.getState().toggle(rowKeyId(oldest.key));

    render(<MessageList />);
    fireEvent.click(screen.getByText('Compiler design notes'));
    fireEvent.click(screen.getByRole('button', { name: 'Select All' }));

    const selected = useListStore.getState().selectedKeys;
    expect(selected).toHaveLength(4);
    for (const r of [newest, middle, oldest, fourth]) {
      expect(selected.some((key) => JSON.stringify(key) === JSON.stringify(r.key))).toBe(true);
    }
  });

  it('keeps Show more while the window is full, pins or none', () => {
    applyList({ rows: [newest, middle, oldest, fourth], visible_limit: 4 });
    usePinnedRowsStore.getState().toggle(rowKeyId(oldest.key));

    render(<MessageList />);

    fireEvent.click(screen.getByRole('button', { name: 'Show more' }));
    expect(ipc.dispatch).toHaveBeenCalledWith('show_more');

    // A window the rows did not fill hides the affordance, pins notwithstanding.
    act(() => {
      applyList({ rows: [newest, middle, oldest], visible_limit: 4 });
    });
    expect(screen.queryByRole('button', { name: 'Show more' })).toBeNull();
  });

  it('leaves the search results order alone', () => {
    applyList({
      rows: [newest],
      visible_limit: 50,
      search: {
        query: 'compiler',
        scope: 'all_except_trash',
        rows: [middle, oldest, fourth],
        depth_months: 3,
      },
    });
    usePinnedRowsStore.getState().toggle(rowKeyId(fourth.key));

    render(<MessageList />);

    // The pinned hit keeps its search rank; the overlay is folder-only.
    const order = within(screen.getByRole('listbox'))
      .getAllByRole('option')
      .map((option) => SUBJECTS.find((s) => option.textContent?.includes(s)) ?? '?');
    expect(order).toEqual(['Your weekly digest', 'Compiler design notes', 'Lunch on Friday']);
  });

  it('floats only the pinned account row when two accounts share a key', () => {
    const homeRow = row({
      key: { message: { account: 'home', key: 'imap:v1:u1' } },
      account_id: 'home',
      subject: 'Home mail',
    });
    applyList({ rows: [newest, homeRow], visible_limit: 50 });
    usePinnedRowsStore.getState().toggle(rowKeyId(homeRow.key));

    render(<MessageList />);

    const order = within(screen.getByRole('listbox'))
      .getAllByRole('option')
      .map((option) => option.textContent?.includes('Home mail') ? 'Home mail' : 'Quarterly report');
    expect(order).toEqual(['Home mail', 'Quarterly report']);
    expect(within(optionWith('Home mail')).getByTitle('Pinned')).toBeTruthy();
    expect(within(optionWith('Quarterly report')).queryByTitle('Pinned')).toBeNull();
  });
});
