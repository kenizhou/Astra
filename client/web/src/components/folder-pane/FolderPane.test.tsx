// SPDX-License-Identifier: GPL-3.0-only
// The folder pane renders the mailbox list snapshot exactly as the wire sends
// it: every account group, nesting hidden behind an expanded parent, unread
// badges only where mail is unread, and selection and expansion crossing back
// as intents. Each expanded account's group closes with its Outbox row, which
// badges the sends still owed and leads the list's queue view.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
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
import { useSettingsStore } from '@/features/settings/settingsStore';
import { useUiStore } from '@/stores/uiStore';
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

describe('the Unified inbox row', () => {
  it('leads the pane with the summed badge when two or more accounts exist', () => {
    render(<FolderPane />);

    const row = screen.getByRole('treeitem', { name: /Unified inbox/ });
    expect(row.getAttribute('aria-disabled')).toBeNull();
    const expected = fixture.account_folders
      .flatMap((a) => a.folders)
      .filter((f) => f.role !== 'junk' && f.role !== 'trash')
      .reduce((total, f) => total + f.unread, 0);
    expect(row.textContent).toContain(String(expected));
  });

  it('stays away while fewer than two accounts exist', () => {
    useFolderStore.setState({
      accounts: [fixture.account_folders[0]!],
      selected: null,
      outboxAccount: null,
    });
    render(<FolderPane />);

    expect(screen.queryByRole('treeitem', { name: /Unified inbox/ })).toBeNull();
  });

  it('selects the unified scope on click, so the core persists it', () => {
    render(<FolderPane />);

    const row = screen.getByRole('treeitem', { name: /Unified inbox/ });
    fireEvent.click(row);

    expect(ipc.dispatch).toHaveBeenCalledWith({ select_account: null });
    expect(useFolderStore.getState().unified).toBe(true);
    expect(useFolderStore.getState().selected).toBeNull();
    expect(row.getAttribute('aria-selected')).toBe('true');
  });
});

describe('the account dots', () => {
  it('marks each account header with its palette dot', () => {
    render(<FolderPane />);

    const work = screen.getByRole('region', { name: 'work@example.net' });
    const dots = work.querySelectorAll('[data-account-dot]');
    expect(dots).toHaveLength(1);
    expect((dots[0] as HTMLElement).style.background).toContain('--account-');
  });
});

describe('the per-account refusal badge', () => {
  it('badges an account whose calendar is withheld and opens the accounts page on tap', () => {
    useSettingsStore.getState().applySnapshot({
      settings: {
        theme: 'follow_host',
        twenty_four_clock: false,
        zone: 'Etc/UTC',
        first_day_of_week: 'mo',
        horizon_days: 42,
        quote_below: true,
        view_mode: 'flat',
      },
      accounts: [
        {
          account: 'work',
          address: 'work@example.net',
          kind: 'microsoft',
          legs: { calendar: false, contacts: false },
          grant: 'calendar_withheld',
        },
      ],
      about: { version: '0.1.0' },
    });
    render(<FolderPane />);

    const badge = screen.getByRole('button', { name: 'Reconnect work@example.net' });
    expect(badge.textContent).toContain('calendar');

    fireEvent.click(badge);

    expect(useUiStore.getState().settingsOpen).toBe(true);
    expect(useSettingsStore.getState().requestedCategory).toBe('accounts');
  });

  it('draws no badge for a healthy account', () => {
    useSettingsStore.getState().applySnapshot({
      settings: {
        theme: 'follow_host',
        twenty_four_clock: false,
        zone: 'Etc/UTC',
        first_day_of_week: 'mo',
        horizon_days: 42,
        quote_below: true,
        view_mode: 'flat',
      },
      accounts: [],
      about: { version: '0.1.0' },
    });
    render(<FolderPane />);

    expect(screen.queryByRole('button', { name: 'Reconnect work@example.net' })).toBeNull();
  });
});

describe('the folder context menu', () => {
  /** The fixture's accounts carry no capability flag, so a test that wants
   *  the live verbs stamps the work account as an editor first. */
  const workCanEdit = (): void => {
    useFolderStore.setState((state) => ({
      accounts: state.accounts.map((a) =>
        a.account_id === 'work' ? { ...a, can_edit_folders: true } : a,
      ),
    }));
  };

  it('opens on right-click with the live verbs for an editing account', () => {
    workCanEdit();
    render(<FolderPane />);

    fireEvent.contextMenu(screen.getByText('Acme'));

    expect(screen.getByRole('menu')).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Sync this folder' })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'New Subfolder' })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Rename Folder' }).getAttribute('aria-disabled')).toBeNull();
    expect(screen.getByRole('menuitem', { name: 'Delete Folder' }).getAttribute('aria-disabled')).toBeNull();
    // Mark All as Read stands only for the folder the list is showing.
    expect(screen.queryByRole('menuitem', { name: 'Mark All as Read' })).toBeNull();
  });

  it('hides the tree-edit verbs when the account cannot edit its folders', () => {
    render(<FolderPane />);

    fireEvent.contextMenu(screen.getByText('Acme'));

    expect(screen.getByRole('menuitem', { name: 'Sync this folder' })).toBeTruthy();
    expect(screen.queryByRole('menuitem', { name: 'New Subfolder' })).toBeNull();
    expect(screen.queryByRole('menuitem', { name: 'Rename Folder' })).toBeNull();
    expect(screen.queryByRole('menuitem', { name: 'Delete Folder' })).toBeNull();
  });

  it('syncs from the menu', () => {
    render(<FolderPane />);

    fireEvent.contextMenu(screen.getByText('Acme'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Sync this folder' }));

    expect(ipc.dispatch).toHaveBeenCalledWith('refresh_mail');
  });

  it('stands the inline create row under the folder and commits the parent', async () => {
    workCanEdit();
    render(<FolderPane />);

    fireEvent.contextMenu(screen.getByText('Clients'));
    fireEvent.click(screen.getByRole('menuitem', { name: 'New Subfolder' }));

    const input = screen.getByLabelText('New folder name');
    expect(document.activeElement).toBe(input);
    fireEvent.change(input, { target: { value: 'Invoices' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(ipc.dispatch).toHaveBeenCalledWith({
      folders: { create: { account: 'work', name: 'Invoices', parent: 'Clients' } },
    });
    // The row closes on the dispatch's settlement, never before.
    await waitFor(() => expect(screen.queryByLabelText('New folder name')).toBeNull());
  });
});
