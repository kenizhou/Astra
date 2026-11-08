// SPDX-License-Identifier: GPL-3.0-only
// The row's avatar column: the sender's monogram at the leading edge,
// resolved per address through the shared avatar cache, replaced by the
// address-book photo when one lands, and hidden at the compact density the
// row's own rules already draw by.
import { cleanup, render, screen, waitFor } from '@testing-library/react';
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

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.peopleAvatar.mockReset();
  ipc.peopleAvatar.mockResolvedValue({ initials: 'AL', colour: '#039be5' });
  ipc.peoplePhoto.mockReset();
  ipc.peoplePhoto.mockResolvedValue(null);
  useListStore.setState({ density: 'normal' });
});

afterEach(cleanup);

describe('the sender avatar column', () => {
  it('draws the monogram at the row leading edge, before the sender', async () => {
    render(
      <MessageRow
        row={row()}
        selected={false}
        now={new Date('2026-10-10T00:00:00Z')}
        onSelect={() => undefined}
      />,
    );

    const avatar = await waitFor(() => {
      const found = screen.getByRole('img', { name: 'Avatar' });
      expect(found.textContent).toContain('AL');
      return found;
    });
    // The column stands at the leading edge: the sender reads after it.
    const sender = screen.getByText('Ada Lovelace');
    expect(
      sender.compareDocumentPosition(avatar) & Node.DOCUMENT_POSITION_PRECEDING,
    ).toBeTruthy();
  });

  it('resolves through the per-address cache, once per row', async () => {
    render(
      <MessageRow
        row={row()}
        selected={false}
        now={new Date('2026-10-10T00:00:00Z')}
        onSelect={() => undefined}
      />,
    );

    await waitFor(() => {
      expect(ipc.peopleAvatar).toHaveBeenCalledTimes(1);
    });
    expect(ipc.peopleAvatar).toHaveBeenCalledWith('ada@example.test');
  });

  it('stands on the placeholder until the photo lands, then wears it', async () => {
    ipc.peopleAvatar.mockResolvedValue({
      initials: 'AL',
      colour: '#039be5',
      photo: 'photos/ada',
    });
    ipc.peoplePhoto.mockResolvedValue('data:image/png;base64,AAAA');
    const { container } = render(
      <MessageRow
        row={row()}
        selected={false}
        now={new Date('2026-10-10T00:00:00Z')}
        onSelect={() => undefined}
      />,
    );

    // The first paint stands on the avatar's own placeholder (the resolve
    // has not answered yet); the row renders whole and never blocks on it.
    const avatar = screen.getByRole('img', { name: 'Avatar' });
    expect(avatar.textContent).not.toContain('AL');
    expect(screen.getByText('Ada Lovelace')).toBeTruthy();

    // The reads answer: the photo replaces the placeholder, the monogram
    // never flashing in between because the two resolves land together.
    await waitFor(() => {
      expect(container.querySelector('img[src="data:image/png;base64,AAAA"]')).not.toBeNull();
    });
  });

  it('hides the column at the compact density', () => {
    useListStore.setState({ density: 'compact' });
    render(
      <MessageRow
        row={row()}
        selected={false}
        now={new Date('2026-10-10T00:00:00Z')}
        onSelect={() => undefined}
      />,
    );

    expect(screen.queryByRole('img', { name: 'Avatar' })).toBeNull();
  });
});
