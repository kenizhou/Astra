// SPDX-License-Identifier: GPL-3.0-only
// The message list's multi-selection (ctrl/shift/ctrl+a semantics, the
// folder-switch reset) and the row context menu (targets on right-click,
// the batch loops, the draft folder's own menu). The list's main tests
// live in MessageList.test.tsx, its groups in MessageList.groups.test.tsx.
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

// jsdom reports no viewport; the mock virtualizer returns every item.
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

/** The listbox option showing one text. */
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
});

describe('the multi-selection', () => {
  const second = row({
    key: { message: { account: 'work', key: 'imap:v1:u5' } },
    subject: 'Second note',
    unread: false,
  });
  const third = row({
    key: { message: { account: 'work', key: 'imap:v1:u6' } },
    subject: 'Third note',
    unread: false,
  });

  it('ctrl-click toggles rows without opening them', () => {
    applyList({ rows: [row(), second, third], visible_limit: 50 });

    render(<MessageList />);
    fireEvent.click(screen.getByText('Quarterly report'));
    ipc.dispatch.mockClear();

    fireEvent.click(screen.getByText('Second note'), { ctrlKey: true });
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(useListStore.getState().selectedKeys).toHaveLength(2);

    fireEvent.click(screen.getByText('Second note'), { ctrlKey: true });
    expect(useListStore.getState().selectedKeys).toHaveLength(1);
  });

  it('shift-click ranges from the anchor, headers excluded', () => {
    applyList({ rows: [row(), digest, second, third], visible_limit: 50 });

    render(<MessageList />);
    fireEvent.click(screen.getByText('Quarterly report'));
    fireEvent.click(screen.getByText('Third note'), { shiftKey: true });

    const keys = useListStore.getState().selectedKeys;
    expect(keys).toHaveLength(4);
    // Every key is a message key: the date headers never count.
    expect(keys.every((key) => 'message' in key)).toBe(true);
  });

  it('ctrl+a selects every loaded row', () => {
    applyList({ rows: [row(), second, third], visible_limit: 50 });

    render(<MessageList />);
    fireEvent.keyDown(screen.getByLabelText('Message list'), { key: 'a', ctrlKey: true });

    expect(useListStore.getState().selectedKeys).toHaveLength(3);
  });

  it('clears the multi-selection on a folder switch', () => {
    applyList({ rows: [row(), second], visible_limit: 50 });

    render(<MessageList />);
    fireEvent.keyDown(screen.getByLabelText('Message list'), { key: 'a', ctrlKey: true });
    expect(useListStore.getState().selectedKeys).toHaveLength(2);

    act(() => {
      useFolderStore.setState({ selected: { account: 'work', key: 'Sent' } });
    });
    expect(useListStore.getState().selectedKeys).toHaveLength(0);
  });
});

describe('the row context menu', () => {
  const second = row({
    key: { message: { account: 'work', key: 'imap:v1:u5' } },
    subject: 'Second note',
    unread: false,
  });

  it('opens on right-click with the clicked row as the target', () => {
    applyList({ rows: [row(), second], visible_limit: 50 });

    render(<MessageList />);
    fireEvent.contextMenu(optionWith('Second note'));

    expect(screen.getByRole('menu')).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Archive' })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Delete' })).toBeTruthy();
    expect(useListStore.getState().selectedKeys).toEqual([
      { message: { account: 'work', key: 'imap:v1:u5' } },
    ]);
  });

  it('keeps a multi-selection as the targets when the click lands inside it', () => {
    applyList({ rows: [row(), second], visible_limit: 50 });

    render(<MessageList />);
    fireEvent.click(screen.getByText('Quarterly report'));
    fireEvent.click(screen.getByText('Second note'), { ctrlKey: true });
    fireEvent.contextMenu(optionWith('Second note'));

    expect(screen.getByRole('menuitem', { name: 'Archive 2 messages' })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Delete 2 messages' })).toBeTruthy();
  });

  it('loops the batch archive over every target', () => {
    applyList({ rows: [row(), second], visible_limit: 50 });

    render(<MessageList />);
    fireEvent.click(screen.getByText('Quarterly report'));
    fireEvent.click(screen.getByText('Second note'), { ctrlKey: true });
    ipc.dispatch.mockClear();
    fireEvent.contextMenu(optionWith('Second note'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Archive 2 messages' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      archive_message: { row: { message: { account: 'work', key: 'imap:v1:u1' } } },
    });
    expect(ipc.dispatch).toHaveBeenCalledWith({
      archive_message: { row: { message: { account: 'work', key: 'imap:v1:u5' } } },
    });
  });

  it('closes on Escape', () => {
    applyList({ rows: [row()], visible_limit: 50 });

    render(<MessageList />);
    fireEvent.contextMenu(optionWith('Quarterly report'));
    expect(screen.getByRole('menu')).toBeTruthy();

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('shows the draft menu in the drafts folder', () => {
    useFolderStore.setState({
      accounts: [
        {
          account_id: 'work',
          display_name: 'work@example.net',
          address: 'work@example.net',
          expanded: true,
          folders: [
            {
              key: 'Drafts',
              name: 'Drafts',
              role: 'drafts',
              unread: 1,
              depth: 0,
              has_children: false,
              expanded: true,
            },
          ],
        },
      ],
      selected: { account: 'work', key: 'Drafts' },
    });
    applyList({ rows: [row()], visible_limit: 50 });

    render(<MessageList />);
    expect(screen.getByText('Draft')).toBeTruthy();

    fireEvent.contextMenu(optionWith('Quarterly report'));
    expect(screen.getByRole('menuitem', { name: 'Open' })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Delete Draft' })).toBeTruthy();
    expect(screen.queryByRole('menuitem', { name: 'Archive' })).toBeNull();
  });
});
