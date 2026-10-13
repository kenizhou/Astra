// SPDX-License-Identifier: GPL-3.0-only
// The folder pane renders the mailbox list snapshot exactly as the wire sends
// it: every account group, nesting hidden behind an expanded parent, unread
// badges only where mail is unread, and selection and expansion crossing back
// as intents. Each expanded account's group closes with its Outbox row, which
// badges the sends still owed and leads the list's queue view.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
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

import { FolderPane } from './FolderPane';
import { useFolderStore } from '@/stores/folderStore';
import { useSendingStore } from '@/stores/sendingStore';
import type { MailboxListSnapshot } from '@/services/surfaces';
import fixtureJson from '@/services/__fixtures__/mailbox_list.json';

const fixture = fixtureJson as MailboxListSnapshot;

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useFolderStore.setState({ accounts: [], selected: null, outboxAccount: null });
  useFolderStore.getState().applySnapshot(fixture);
  useSendingStore.getState().applySnapshot({ rows: [] });
});

afterEach(() => {
  cleanup();
});

describe('the folder pane', () => {
  it('renders every account group with its folders', () => {
    render(<FolderPane />);

    expect(screen.getByText('work@example.net')).toBeTruthy();
    expect(screen.getByText('Home')).toBeTruthy();
    expect(screen.getByText('Inbox')).toBeTruthy();
    expect(screen.getByText('Clients')).toBeTruthy();
    expect(screen.getByText('Acme')).toBeTruthy();
    expect(screen.getByText('Sent')).toBeTruthy();
  });

  it('hides a collapsed account folder tree and an unexpanded parent', () => {
    render(<FolderPane />);

    // The Home account is collapsed, so its single Inbox has no row; the
    // fixture carries one Inbox per account, so only one row renders.
    expect(screen.getAllByText('Inbox')).toHaveLength(1);
  });

  it('badges unread counts and draws none at zero', () => {
    render(<FolderPane />);

    expect(screen.getByText('3')).toBeTruthy();
    expect(screen.getByText('2')).toBeTruthy();
    expect(screen.getByText('7')).toBeTruthy();
    // Clients and Sent sit at zero unread: no badge node for them.
    expect(screen.queryByText('0')).toBeNull();
  });

  it('selecting a folder dispatches the select_folder intent and marks the row', () => {
    render(<FolderPane />);

    fireEvent.click(screen.getByText('Acme'));

    expect(ipc.dispatch).toHaveBeenCalledWith(
      expect.objectContaining({
        select_folder: { folder: { account: 'work', key: 'Clients/Acme' } },
      }),
    );
    expect(useFolderStore.getState().selected).toEqual({
      account: 'work',
      key: 'Clients/Acme',
    });
    expect(screen.getByText('Acme').closest('[role="treeitem"]')?.getAttribute('aria-selected')).toBe(
      'true',
    );
  });

  it('a folder chevron dispatches set_folder_expanded for that folder', () => {
    render(<FolderPane />);

    fireEvent.click(screen.getByRole('button', { name: 'Collapse Clients' }));

    expect(ipc.dispatch).toHaveBeenCalledWith(
      expect.objectContaining({
        set_folder_expanded: { folder: { account: 'work', key: 'Clients' }, expanded: false },
      }),
    );
  });

  it('an account chevron dispatches set_account_expanded', () => {
    render(<FolderPane />);

    fireEvent.click(screen.getByRole('button', { name: 'Collapse work@example.net' }));

    expect(ipc.dispatch).toHaveBeenCalledWith(
      expect.objectContaining({
        set_account_expanded: { account: 'work', expanded: false },
      }),
    );
  });
});

describe('the outbox row', () => {
  it('closes every expanded account group, with no badge at zero', () => {
    render(<FolderPane />);

    // The Home account is collapsed, so only the work group shows its row.
    const rows = screen.getAllByText('Outbox');
    expect(rows).toHaveLength(1);
    expect(rows[0]?.closest('[role="treeitem"]')?.getAttribute('aria-selected')).toBe('false');
    expect(screen.queryByText('0')).toBeNull();
  });

  it('badges the account sends still owed to the server', () => {
    useSendingStore.getState().applySnapshot({
      rows: [
        { queued: { account: 'work', op: 1 }, state: 'queued', subject: 'One', arrived_at: '' },
        { queued: { account: 'work', op: 2 }, state: 'sending', subject: 'Two', arrived_at: '' },
        { queued: { account: 'work', op: 3 }, state: 'failed', subject: 'Three', arrived_at: '' },
        { queued: { account: 'home', op: 4 }, state: 'queued', subject: 'Four', arrived_at: '' },
      ],
    });
    render(<FolderPane />);

    const row = screen.getByText('Outbox').closest('[role="treeitem"]');
    // Queued and in flight count; the refused send and the other account's
    // queue do not.
    expect(row?.textContent).toContain('2');
  });

  it('selecting it records the account and asks the host for the queue', () => {
    render(<FolderPane />);

    fireEvent.click(screen.getByText('Outbox'));

    expect(useFolderStore.getState().outboxAccount).toBe('work');
    expect(useFolderStore.getState().selected).toBeNull();
    expect(ipc.dispatch).toHaveBeenCalledWith({ outbox: 'show' });
    expect(
      screen.getByText('Outbox').closest('[role="treeitem"]')?.getAttribute('aria-selected'),
    ).toBe('true');
  });

  it('a folder selection clears the outbox selection again', () => {
    render(<FolderPane />);

    fireEvent.click(screen.getByText('Outbox'));
    expect(useFolderStore.getState().outboxAccount).toBe('work');

    fireEvent.click(screen.getByText('Sent'));
    expect(useFolderStore.getState().outboxAccount).toBeNull();
    expect(useFolderStore.getState().selected).toEqual({ account: 'work', key: 'Sent' });
  });
});
