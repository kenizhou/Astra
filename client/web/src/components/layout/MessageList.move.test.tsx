// SPDX-License-Identifier: GPL-3.0-only
// The row menu's move-to verb over the live wiring: the context the pane
// builds and the item the builder answers, together. A selection that
// spans accounts hides the verb outright (no placeholder misnaming a
// shipped feature), and a conversation target disables it with the
// reason, matching the compose verbs' honest-disable channel.
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

// jsdom reports no viewport, so the windowed virtualizer would render
// nothing; the mock returns every item, as the main list file's does.
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
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('the row menu\'s move-to verb', () => {
  it('hides the verb for a selection that spans accounts, no placeholder', () => {
    const home = row({
      key: { message: { account: 'home', key: 'imap:v1:h1' } },
      account_id: 'home',
      subject: 'Elsewhere',
    });
    applyList({ rows: [row(), home] });
    render(<MessageList />);
    act(() => {
      useListStore.setState({ selectedKeys: [row().key, home.key], anchorKey: row().key });
    });

    fireEvent.contextMenu(optionWith('Quarterly report'));

    expect(screen.queryByRole('menuitem', { name: 'Move to folder' })).toBeNull();
  });

  it('disables the verb with the conversation reason on a thread row', () => {
    const thread = row({
      key: { thread: { account: 'work', thread_id: 'th-1' } },
      subject: 'Quarterly thread',
      thread_count: 2,
    });
    applyList({ rows: [thread] });
    render(<MessageList />);
    act(() => {
      useListStore.setState({ selectedKeys: [thread.key], anchorKey: thread.key });
    });

    fireEvent.contextMenu(optionWith('Quarterly thread'));

    const item = screen.getByRole('menuitem', { name: 'Move to folder' });
    expect(item.getAttribute('aria-disabled')).toBe('true');
    expect(screen.getByText('A whole conversation cannot be moved for this yet')).toBeTruthy();
  });
});
