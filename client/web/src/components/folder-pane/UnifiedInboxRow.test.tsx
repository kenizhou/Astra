// SPDX-License-Identifier: GPL-3.0-only
// The unified inbox row: live now that the core serves the scope. Clicking
// selects the unified view (the select-account-null family, persisted by the
// core), the unread badge keeps counting, and the row still renders only
// while the pane holds at least two accounts.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { UnifiedInboxRow } from './UnifiedInboxRow';
import type { AccountFolderRow } from '@/services/surfaces';

afterEach(() => {
  cleanup();
  ipc.dispatch.mockReset();
});

function account(id: string, unread = 0): AccountFolderRow {
  return {
    account_id: id,
    display_name: id,
    address: `${id}@example.test`,
    expanded: true,
    folders: [
      { key: 'INBOX', name: 'Inbox', role: 'inbox', unread, depth: 0, has_children: false, expanded: false },
    ],
  };
}

describe('the unified inbox row', () => {
  it('selects the unified scope on click', () => {
    render(<UnifiedInboxRow accounts={[account('work', 3), account('home')]} />);

    fireEvent.click(screen.getByRole('treeitem', { name: /Unified inbox/ }));

    expect(ipc.dispatch).toHaveBeenCalledWith({ select_account: null });
  });

  it('draws as a live row: no preview tooltip, no disabled claim', () => {
    render(<UnifiedInboxRow accounts={[account('work'), account('home')]} />);

    const row = screen.getByRole('treeitem', { name: /Unified inbox/ });
    expect(row.getAttribute('aria-disabled')).toBeNull();
    expect(row.getAttribute('title')).toBeNull();
  });

  it('keeps the unread badge over both accounts', () => {
    render(<UnifiedInboxRow accounts={[account('work', 3), account('home', 4)]} />);

    expect(screen.getByText('7')).toBeTruthy();
  });

  it('stays hidden with one account', () => {
    const { container } = render(<UnifiedInboxRow accounts={[account('work')]} />);
    expect(container.firstChild).toBeNull();
  });
});
