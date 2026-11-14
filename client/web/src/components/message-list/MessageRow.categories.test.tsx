// SPDX-License-Identifier: GPL-3.0-only
// The row's leading colour bar: the first category's colour, the tooltip
// naming every category the row carries, and a deleted definition resolving
// to nothing (its id drops out of the bar). The defs ride the real store.
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  peopleAvatar: vi.fn(),
  peoplePhoto: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  peopleAvatar: ipc.peopleAvatar,
  peoplePhoto: ipc.peoplePhoto,
}));

import { MessageRow } from './MessageRow';
import { useCategoriesStore } from '@/features/categories/categoriesStore';
import { useListStore } from '@/stores/listStore';
import type { MessageRowView, RowKey } from '@/services/surfaces';

const messageKey: RowKey = { message: { account: 'work', key: 'k1' } };

function row(over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key: messageKey,
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

function renderRow(over: Partial<MessageRowView> = {}) {
  return render(
    <MessageRow
      row={row(over)}
      selected={false}
      now={new Date('2026-10-10T00:00:00Z')}
      onSelect={() => undefined}
    />,
  );
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.peopleAvatar.mockReset();
  ipc.peopleAvatar.mockResolvedValue({ initials: 'AL', colour: '#039be5' });
  ipc.peoplePhoto.mockReset();
  ipc.peoplePhoto.mockResolvedValue(null);
  localStorage.clear();
  useListStore.setState({ density: 'normal' });
  useCategoriesStore.setState({
    defs: [
      { id: 'work', name: 'Work', color: '#FF0000' },
      { id: 'personal', name: 'Personal', color: '#00FF00' },
    ],
    revision: 0,
    loaded: true,
    dialogOpen: false,
  });
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe('the row colour bar', () => {
  it('wears the first category colour and names every category in the tooltip', () => {
    renderRow({ categories: ['work', 'personal'] });
    const bar = screen.getByTitle('Work, Personal');
    expect(bar.getAttribute('style')).toContain('rgb(255, 0, 0)');
  });

  it('is absent on a row carrying no category', () => {
    renderRow();
    expect(screen.queryByTitle('Work')).toBeNull();
  });

  it('resolves a deleted definition to nothing', () => {
    renderRow({ categories: ['ghost'] });
    // No def matches the id, so the bar never stands.
    expect(screen.queryByTitle('Work')).toBeNull();
    expect(screen.queryByTitle('ghost')).toBeNull();
  });

  it('skips a deleted definition and colours from the first that resolves', () => {
    renderRow({ categories: ['ghost', 'personal'] });
    const bar = screen.getByTitle('Personal');
    expect(bar.getAttribute('style')).toContain('rgb(0, 255, 0)');
  });
});