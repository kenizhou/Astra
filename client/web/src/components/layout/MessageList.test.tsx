// SPDX-License-Identifier: GPL-3.0-only
// The message list pane: rows render from a snapshot through the
// virtualizer, the states a row draws (bold, flag, attachments, thread
// badge, stamp), the selection (a single click selects and opens, the
// highlight following the open message), every intent crossing back
// (show more, view mode, mark read, set flagged, open message) with its
// exact payload, and the split filter (the badges, the side views, the
// unmapped rows riding All only, and the selection reset).
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
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
import type { MailboxListSnapshot, MessageRowView } from '@/services/surfaces';
import type { ReadingSnapshot } from '@/services/ipc';
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
  });
  useListStore.setState({
    rows: [],
    viewMode: 'flat',
    visibleLimit: 0,
    selection: null,
    selectedKey: null,
  });
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useSplitStore.setState({ entries: {}, filter: 'all' });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('the message list', () => {
  it('renders the snapshot rows through the virtualizer', () => {
    applyList({ rows: [digest, row()], visible_limit: 50 });

    render(<MessageList />);

    // The first two rows are present: sender, subject, and stamp each way.
    expect(screen.getByText('Ada Lovelace')).toBeTruthy();
    expect(screen.getByText('Quarterly report')).toBeTruthy();
    expect(screen.getByText('08:30')).toBeTruthy();
    // A row with no sender name shows the bare address, and an earlier day
    // of the same year renders as day and month.
    expect(screen.getByText('notifications@example.test')).toBeTruthy();
    expect(screen.getByText('Your weekly digest')).toBeTruthy();
    expect(screen.getByText('8 Oct')).toBeTruthy();
    expect(screen.getAllByRole('option')).toHaveLength(2);
    // The toolbar names the open folder.
    expect(screen.getByText('Inbox')).toBeTruthy();
  });

  it('draws unread rows bold and read rows plain', () => {
    applyList({ rows: [row({ unread: true }), digest], visible_limit: 50 });

    render(<MessageList />);

    expect(screen.getByText('Quarterly report').className).toContain('font-semibold');
    expect(screen.getByText('Your weekly digest').className).not.toContain('font-semibold');
  });

  it('draws the sender monogram from the address initial', () => {
    applyList({ rows: [digest], visible_limit: 50 });

    render(<MessageList />);

    expect(within(optionWith('Your weekly digest')).getByText('N')).toBeTruthy();
  });

  it('shows the flag and attachment glyphs on a flagged row', () => {
    applyList({ rows: [row({ flagged: true, has_attachments: true }), digest], visible_limit: 50 });

    render(<MessageList />);

    const flagged = screen.getByRole('button', { name: 'Unflag' });
    expect(flagged.getAttribute('aria-pressed')).toBe('true');
    expect(within(optionWith('Quarterly report')).getByTitle('Has attachments')).toBeTruthy();
    // An unflagged, unselected row keeps its flag button hidden until hover,
    // and an attachment-free row draws no glyph.
    expect(screen.queryByRole('button', { name: 'Flag' })).toBeNull();
    expect(within(optionWith('Your weekly digest')).queryByTitle('Has attachments')).toBeNull();
  });

  it('badges the conversation size in threaded mode', () => {
    applyList({
      rows: [
        row({
          key: { thread: { account: 'work', thread_id: 'th-7' } },
          subject: 'Re: Compiler design',
          thread_count: 3,
        }),
      ],
      visible_limit: 50,
      view_mode: 'threaded',
    });

    render(<MessageList />);

    expect(screen.getByTitle('3 messages').textContent).toBe('3');
  });

  it('draws no thread badge in flat mode', () => {
    applyList({ rows: [row()], visible_limit: 50 });

    render(<MessageList />);

    expect(screen.queryByTitle('1 message')).toBeNull();
  });

  it('shows more only while the window is full, and dispatches on click', () => {
    applyList({ rows: [row(), digest], visible_limit: 2 });

    render(<MessageList />);

    fireEvent.click(screen.getByRole('button', { name: 'Show more' }));
    expect(ipc.dispatch).toHaveBeenCalledWith('show_more');

    // A window the rows did not fill hides the affordance: the folder is
    // exhausted.
    act(() => {
      applyList({ rows: [row()], visible_limit: 2 });
    });
    expect(screen.queryByRole('button', { name: 'Show more' })).toBeNull();
  });

  it('dispatches set_view_mode from the segmented control', () => {
    applyList({ rows: [row()], visible_limit: 50 });

    render(<MessageList />);

    expect(screen.getByRole('button', { name: 'Flat' }).getAttribute('aria-pressed')).toBe('true');

    fireEvent.click(screen.getByRole('button', { name: 'Threaded' }));

    expect(ipc.dispatch).toHaveBeenCalledWith({ set_view_mode: 'threaded' });
  });

  it('selects and opens a row on single click with the exact payload', () => {
    applyList({ rows: [row(), digest], visible_limit: 50 });

    render(<MessageList />);

    fireEvent.click(screen.getByText('Ada Lovelace'));

    expect(optionWith('Quarterly report').getAttribute('aria-selected')).toBe('true');
    expect(optionWith('Your weekly digest').getAttribute('aria-selected')).toBe('false');
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      open_message: { message: { account: 'work', key: 'imap:v1:u1' } },
    });
  });

  it('selects a thread row locally without opening anything', () => {
    applyList({
      rows: [
        row({
          key: { thread: { account: 'work', thread_id: 'th-7' } },
          subject: 'Re: Compiler design',
          thread_count: 3,
        }),
      ],
      visible_limit: 50,
      view_mode: 'threaded',
    });

    render(<MessageList />);

    fireEvent.click(screen.getByText('Ada Lovelace'));

    expect(optionWith('Re: Compiler design').getAttribute('aria-selected')).toBe('true');
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('follows the open message with the selection highlight', () => {
    applyList({ rows: [row(), digest], visible_limit: 50 });
    // The reading pull landed for the digest: the highlight follows the open
    // message, ahead of any local click selection.
    const open: ReadingSnapshot = {
      message: { account: 'work', key: 'imap:v1:u2' },
      subject: 'Your weekly digest',
      from_name: '',
      from_address: 'notifications@example.test',
      sent_at: '2026-10-08T16:05:00Z',
      remote_image_count: 0,
    };
    useReadingStore.getState().applySnapshot(open);

    render(<MessageList />);

    expect(optionWith('Quarterly report').getAttribute('aria-selected')).toBe('false');
    expect(optionWith('Your weekly digest').getAttribute('aria-selected')).toBe('true');
  });

  it('marks the selected row read with the exact payload', () => {
    applyList({ rows: [row({ unread: true }), digest], visible_limit: 50 });

    render(<MessageList />);

    fireEvent.click(screen.getByText('Ada Lovelace'));
    fireEvent.click(screen.getByRole('button', { name: 'Mark as read' }));

    expect(ipc.dispatch).toHaveBeenCalledWith({
      mark_read: { row: { message: { account: 'work', key: 'imap:v1:u1' } }, read: true },
    });
  });

  it('marks a read row unread', () => {
    applyList({ rows: [digest], visible_limit: 50 });

    render(<MessageList />);

    fireEvent.click(screen.getByText('Your weekly digest'));
    fireEvent.click(screen.getByRole('button', { name: 'Mark as unread' }));

    expect(ipc.dispatch).toHaveBeenCalledWith({
      mark_read: { row: { message: { account: 'work', key: 'imap:v1:u2' } }, read: false },
    });
  });

  it('flags the selected thread row with the exact payload', () => {
    applyList({
      rows: [
        row({
          key: { thread: { account: 'work', thread_id: 'th-7' } },
          subject: 'Re: Compiler design',
          unread: false,
          thread_count: 3,
        }),
      ],
      visible_limit: 50,
      view_mode: 'threaded',
    });

    render(<MessageList />);

    fireEvent.click(screen.getByText('Ada Lovelace'));
    fireEvent.click(screen.getByRole('button', { name: 'Flag' }));

    expect(ipc.dispatch).toHaveBeenCalledWith({
      set_flagged: { row: { thread: { account: 'work', thread_id: 'th-7' } }, flagged: true },
    });
  });

  it('unflags a flagged row from its own glyph', () => {
    applyList({ rows: [row({ flagged: true })], visible_limit: 50 });

    render(<MessageList />);

    fireEvent.click(screen.getByRole('button', { name: 'Unflag' }));

    expect(ipc.dispatch).toHaveBeenCalledWith({
      set_flagged: { row: { message: { account: 'work', key: 'imap:v1:u1' } }, flagged: false },
    });
  });

  it('renders an empty state for an empty folder', () => {
    applyList({ rows: [], visible_limit: 0 });

    render(<MessageList />);

    expect(screen.getByText('No messages in this folder.')).toBeTruthy();
    expect(screen.queryByRole('option')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Show more' })).toBeNull();
  });

  it('renders a prompt while no folder is selected', () => {
    useFolderStore.setState({ selected: null });
    applyList({ rows: [row()], visible_limit: 50 });

    render(<MessageList />);

    expect(screen.getByText('Select a folder to read its mail.')).toBeTruthy();
    expect(screen.queryByRole('option')).toBeNull();
  });
});

describe('the split filter', () => {
  /** The actioned row: a stored primary message. */
  const signOff = row({
    key: { message: { account: 'work', key: 'imap:v1:u7' } },
    subject: 'Needs your sign off',
    unread: false,
  });

  /** The FYI row: a stored newsletter. */
  const newsletter = row({
    key: { message: { account: 'work', key: 'imap:v1:u8' } },
    subject: 'Your weekly digest',
    unread: false,
  });

  /** Applies the split map matching the rows above: u7 actioned, u8 FYI, u1 unmapped. */
  function applySplit(): void {
    useSplitStore.getState().applyMap('work', [
      { key: 'imap:v1:u7', side: 'actioned' },
      { key: 'imap:v1:u8', side: 'fyi' },
    ]);
  }

  it('carries the counts per side on the badges', () => {
    applySplit();
    applyList({ rows: [row(), signOff, newsletter], visible_limit: 50 });

    render(<MessageList />);

    expect(screen.getByRole('button', { name: 'All 3' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Actioned 1' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'FYI 1' })).toBeTruthy();
  });

  it('filters the rows to the chosen side, unmapped rows riding All only', () => {
    applySplit();
    applyList({ rows: [row(), signOff, newsletter], visible_limit: 50 });

    render(<MessageList />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Actioned 1' }));
    });
    expect(screen.getAllByRole('option')).toHaveLength(1);
    expect(screen.getByText('Needs your sign off')).toBeTruthy();
    expect(screen.queryByText('Quarterly report')).toBeNull();
    expect(screen.queryByText('Your weekly digest')).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'FYI 1' }));
    });
    expect(screen.getAllByRole('option')).toHaveLength(1);
    expect(screen.getByText('Your weekly digest')).toBeTruthy();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'All 3' }));
    });
    expect(screen.getAllByRole('option')).toHaveLength(3);
    expect(screen.getByText('Quarterly report')).toBeTruthy();
  });

  it('resets the selection when the filter hides the selected row', () => {
    applySplit();
    applyList({ rows: [row(), signOff], visible_limit: 50 });

    render(<MessageList />);
    fireEvent.click(screen.getByText('Quarterly report'));
    expect(useListStore.getState().selectedKey).toEqual({ message: { account: 'work', key: 'imap:v1:u1' } });

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Actioned 1' }));
    });

    // The unmapped row is gone from the side view, so its selection went
    // with it: nothing stays selected, and All returning does not restore it.
    expect(useListStore.getState().selectedKey).toBeNull();
    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'All 2' }));
    });
    expect(optionWith('Quarterly report').getAttribute('aria-selected')).toBe('false');
  });

  it('keeps the selection when the filter keeps the row', () => {
    applySplit();
    applyList({ rows: [row(), signOff], visible_limit: 50 });

    render(<MessageList />);
    fireEvent.click(screen.getByText('Needs your sign off'));

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Actioned 1' }));
    });

    expect(optionWith('Needs your sign off').getAttribute('aria-selected')).toBe('true');
    expect(useListStore.getState().selectedKey).toEqual({ message: { account: 'work', key: 'imap:v1:u7' } });
  });

  it('shows an empty state when the side holds none of the rows', () => {
    applyList({ rows: [row()], visible_limit: 50 });

    render(<MessageList />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Actioned 0' }));
    });

    expect(screen.getByText('No messages on this side.')).toBeTruthy();
    expect(screen.queryByRole('option')).toBeNull();
    // The folder itself is not empty, so the folder's own empty state stays away.
    expect(screen.queryByText('No messages in this folder.')).toBeNull();
  });

  it('keeps Show more while the window is full even when a side shows fewer rows', () => {
    applySplit();
    applyList({ rows: [row(), signOff, newsletter], visible_limit: 3 });

    render(<MessageList />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Actioned 1' }));
    });

    // Rows behind the load window may still belong to the side, so a full
    // window keeps offering to grow it.
    expect(screen.getByRole('button', { name: 'Show more' })).toBeTruthy();
  });
});
