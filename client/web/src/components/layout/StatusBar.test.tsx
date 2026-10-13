// SPDX-License-Identifier: GPL-3.0-only
// The status bar: a sync lamp click asks for a refresh, the send lamp
// stands while the outbox owes sends, and the right side counts the list's
// selection only while one stands.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { StatusBar } from './StatusBar';
import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useSendingStore } from '@/stores/sendingStore';
import { useUiStore } from '@/stores/uiStore';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useUiStore.setState({ engineState: 'running', sync: [] });
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useFolderStore.setState({ selected: null });
  useListStore.setState({ rows: [], selectedKey: null });
  useSendingStore.getState().applySnapshot({ rows: [] });
});

afterEach(cleanup);

describe('the sync lamps', () => {
  it('dispatch refresh_mail on a click', () => {
    useUiStore.getState().applySyncProgress({
      accounts: [{ account: 'work', state: 'idle' }],
    });
    render(<StatusBar />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Work up to date' }));
    });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith('refresh_mail');
  });

  it('keeps the engine startup and the no-sync note unclickable', () => {
    useUiStore.setState({ engineState: 'starting' });
    render(<StatusBar />);
    expect(screen.getByText('Starting')).toBeTruthy();
    expect(screen.queryByRole('button')).toBeNull();
  });
});

describe('the send lamp', () => {
  it('stands while the outbox owes sends, counting queued and in flight', () => {
    useSendingStore.getState().applySnapshot({
      rows: [
        { queued: { account: 'work', op: 1 }, state: 'queued', subject: 'One', arrived_at: '' },
        { queued: { account: 'work', op: 2 }, state: 'sending', subject: 'Two', arrived_at: '' },
        { queued: { account: 'work', op: 3 }, state: 'failed', subject: 'Three', arrived_at: '' },
      ],
    });
    render(<StatusBar />);

    expect(screen.getByText('2 sending')).toBeTruthy();
  });

  it('stays away when every send has gone', () => {
    render(<StatusBar />);

    expect(screen.queryByText(/sending/)).toBeNull();
  });
});

describe('the selection count', () => {
  it('shows the count only while a row is selected', () => {
    render(<StatusBar />);
    expect(screen.queryByText(/^\d+ selected$/)).toBeNull();

    act(() => {
      useListStore.getState().select({ message: { account: 'work', key: 'imap:v1:u1' } });
    });

    expect(screen.getByText('1 selected')).toBeTruthy();
  });
});
