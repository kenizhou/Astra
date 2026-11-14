// SPDX-License-Identifier: GPL-3.0-only
// The list header's whose-mail label: the open folder's name while a
// folder leads, the account's own label while the switcher's scope
// stands, and All Accounts for the unified view. The scope the store
// mirrors is the snapshot's own word for it, so a relaunch labels the
// same scope the core holds. The flat listing's other assertions live in
// MessageList.test.tsx.
import { act, cleanup, render, screen } from '@testing-library/react';
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
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { useSplitStore } from '@/stores/splitStore';
import type { MailboxListSnapshot, MessageRowView } from '@/services/surfaces';
import fixtureJson from '@/services/__fixtures__/mailbox_list.json';

/** A flat row for the folder-led case. */
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

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-10-09T12:00:00Z'));
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
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

describe('the list header whose mail stands', () => {
  it('names the account whose scope stands', () => {
    applyList({ rows: [], visible_limit: 0 });
    act(() => {
      useFolderStore.setState({
        selected: null,
        scope: { kind: 'account', account: 'work' },
      });
    });

    render(<MessageList />);

    // The header names whose mail stands: the account's own label,
    // exactly the label the pane's header row carries.
    expect(screen.getByText('work@example.net')).toBeTruthy();
  });

  it('names the unified view All Accounts', () => {
    applyList({ rows: [], visible_limit: 0 });
    act(() => {
      useFolderStore.setState({
        selected: null,
        scope: { kind: 'all_inboxes' },
      });
    });

    render(<MessageList />);

    expect(screen.getByText('All Accounts')).toBeTruthy();
  });

  it('keeps naming the open folder while a folder stands selected', () => {
    applyList({ rows: [row()], visible_limit: 50 });

    render(<MessageList />);

    expect(screen.getByText('Inbox')).toBeTruthy();
  });
});
