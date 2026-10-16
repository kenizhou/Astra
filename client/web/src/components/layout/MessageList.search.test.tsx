// SPDX-License-Identifier: GPL-3.0-only
// The message list's search results mode: the snapshot's search state swaps
// the folder rows for the result rows (grouped thread rows with their
// conversation counts through the same row view), carries the result count
// header, the depth note, and the exit affordance, selects and opens a
// result with the same flow the folder rows use, hides the split control
// whose sides derive from the folder the list is not showing, and exits
// back to the folder rows once the clear's snapshot lands.

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
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
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { useSplitStore } from '@/stores/splitStore';
import type { MailboxListSnapshot, MessageRowView, SearchState } from '@/services/surfaces';
import fixtureJson from '@/services/__fixtures__/mailbox_list.json';

/** A flat folder row, the one the search mode must displace. */
function folderRow(): MessageRowView {
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
  };
}

/** A flat search hit, on its own message key. */
const hit: MessageRowView = {
  ...folderRow(),
  key: { message: { account: 'work', key: 'imap:v1:u41' } },
  sender_name: 'Grace Hopper',
  sender_address: 'grace@example.test',
  subject: 'Compiler design notes',
  sent_at: '2026-10-07T09:15:00Z',
  unread: false,
};

/** A grouped thread hit, carrying the conversation count. */
const threadHit: MessageRowView = {
  ...folderRow(),
  key: { thread: { account: 'work', thread_id: 'th-9' } },
  sender_name: 'Grace Hopper',
  sender_address: 'grace@example.test',
  subject: 'Re: Compiler design notes',
  sent_at: '2026-10-06T18:45:00Z',
  unread: false,
  thread_count: 3,
};

/** Applies one list view to the store: the rows, the window, and the mode. */
function applyList(snapshot: Partial<MailboxListSnapshot>): void {
  useListStore.getState().applySnapshot({ account_folders: [], ...snapshot });
}

/** Applies a snapshot whose search state is live, over any folder rows too. */
function applySearch(
  search: Partial<SearchState>,
  over: Partial<MailboxListSnapshot> = {},
): void {
  applyList({
    rows: [folderRow()],
    visible_limit: 50,
    ...over,
    search: { query: 'compiler', scope: 'all_except_trash', rows: [hit], depth_months: 3, ...search },
  });
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
  });
  useListStore.setState({
    rows: [],
    viewMode: 'flat',
    visibleLimit: 0,
    selection: null,
    selectedKey: null,
    search: null,
  });
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useSplitStore.setState({ entries: {} });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('the search results mode', () => {
  it('renders the search rows instead of the folder rows', () => {
    applySearch({ rows: [hit] });

    render(<MessageList />);

    expect(screen.getByText('Compiler design notes')).toBeTruthy();
    expect(screen.getAllByRole('option')).toHaveLength(1);
    // The folder rows the snapshot still carried stay off the screen.
    expect(screen.queryByText('Quarterly report')).toBeNull();
    expect(screen.queryByText('No messages in this folder.')).toBeNull();
  });

  it('carries the count header, the depth note, and the exit affordance', () => {
    applySearch({ rows: [hit, threadHit], depth_months: 6 });

    render(<MessageList />);

    expect(screen.getByText('2 results')).toBeTruthy();
    expect(screen.getByText('Searched the last 6 months')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Exit search' })).toBeTruthy();
  });

  it('writes the singular forms for one result and one month', () => {
    applySearch({ rows: [hit], depth_months: 1 });

    render(<MessageList />);

    expect(screen.getByText('1 result')).toBeTruthy();
    expect(screen.getByText('Searched the last month')).toBeTruthy();
  });

  it('badges the grouped thread rows with their conversation count', () => {
    applySearch({ rows: [threadHit] });

    render(<MessageList />);

    expect(screen.getByTitle('3 messages').textContent).toBe('3');
  });

  it('selects and opens a result on single click with the exact payload', () => {
    applySearch({ rows: [hit, threadHit] });

    render(<MessageList />);

    fireEvent.click(screen.getByText('Compiler design notes'));

    expect(optionWith('Compiler design notes').getAttribute('aria-selected')).toBe('true');
    expect(optionWith('Re: Compiler design notes').getAttribute('aria-selected')).toBe('false');
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      open_message: { message: { account: 'work', key: 'imap:v1:u41' } },
    });
  });

  it('selects a thread result locally without opening anything', () => {
    applySearch({ rows: [threadHit] });

    render(<MessageList />);

    fireEvent.click(screen.getByText('Re: Compiler design notes'));

    expect(optionWith('Re: Compiler design notes').getAttribute('aria-selected')).toBe('true');
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('hides the split control while keeping the view mode control', () => {
    applySearch({ rows: [hit] });

    render(<MessageList />);

    expect(screen.queryByRole('group', { name: 'Split' })).toBeNull();
    expect(screen.getByRole('group', { name: 'View mode' })).toBeTruthy();
  });

  it('offers no Show more over results, however full the folder window was', () => {
    applySearch({ rows: [hit] }, { visible_limit: 1 });

    render(<MessageList />);

    expect(screen.queryByRole('button', { name: 'Show more' })).toBeNull();
  });

  it('exits the search and returns to the folder rows', () => {
    applySearch({ rows: [hit] });

    render(<MessageList />);

    fireEvent.click(screen.getByRole('button', { name: 'Exit search' }));
    expect(ipc.dispatch).toHaveBeenCalledWith({ search: { query: null } });

    // The clear's snapshot lands without a search state; the folder rows it
    // carries are what renders again.
    act(() => {
      applyList({ rows: [folderRow()], visible_limit: 50 });
    });
    expect(screen.getByText('Quarterly report')).toBeTruthy();
    expect(screen.queryByText('Compiler design notes')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Exit search' })).toBeNull();
  });

  it('renders an empty state for a search with no hits', () => {
    applySearch({ rows: [] });

    render(<MessageList />);

    expect(screen.getByText('No results.')).toBeTruthy();
    expect(screen.queryByRole('option')).toBeNull();
  });

  it('keeps the results flat with no date group headers', () => {
    applySearch({ rows: [hit, threadHit] });

    render(<MessageList />);

    // The hits fall within the last week, so the folder listing would head
    // them with their weekdays; the results list never groups.
    expect(screen.queryByText('Today')).toBeNull();
    expect(screen.queryByText('Yesterday')).toBeNull();
    expect(screen.queryByText('Wednesday')).toBeNull();
    expect(screen.queryByText('Tuesday')).toBeNull();
  });
});
