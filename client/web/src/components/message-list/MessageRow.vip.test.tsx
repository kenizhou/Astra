// SPDX-License-Identifier: GPL-3.0-only
// The row's VIP marker: a persistent star on rows from a VIP sender,
// absent otherwise, normalised against the set, and live on a mid-view
// toggle. The badge stands without hover or selection, the pin indicator's
// own idiom.
import { act, cleanup, render, screen } from '@testing-library/react';
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
import { useVipSendersStore } from '@/features/vip/vipSendersStore';
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
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.peopleAvatar.mockReset();
  ipc.peopleAvatar.mockResolvedValue({ initials: 'AL', colour: '#039be5' });
  ipc.peoplePhoto.mockReset();
  ipc.peoplePhoto.mockResolvedValue(null);
  localStorage.clear();
  useVipSendersStore.setState({ vip: new Set() });
  useListStore.setState({ density: 'normal' });
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe('the row VIP marker', () => {
  it('stands on a VIP sender\'s row without hover or selection', () => {
    useVipSendersStore.getState().add('ada@example.test');
    renderRow();

    expect(screen.getByTitle('VIP sender')).toBeTruthy();
  });

  it('is absent on a row from any other sender', () => {
    useVipSendersStore.getState().add('bob@example.test');
    renderRow();

    expect(screen.queryByTitle('VIP sender')).toBeNull();
  });

  it('normalises the row address against the set', () => {
    useVipSendersStore.getState().add('ada@example.test');
    renderRow({ sender_address: 'ADA@Example.test' });

    expect(screen.getByTitle('VIP sender')).toBeTruthy();
  });

  it('follows a mid-view toggle', () => {
    renderRow();
    expect(screen.queryByTitle('VIP sender')).toBeNull();

    act(() => {
      useVipSendersStore.getState().toggle('ada@example.test');
    });
    expect(screen.getByTitle('VIP sender')).toBeTruthy();

    act(() => {
      useVipSendersStore.getState().toggle('ada@example.test');
    });
    expect(screen.queryByTitle('VIP sender')).toBeNull();
  });
});