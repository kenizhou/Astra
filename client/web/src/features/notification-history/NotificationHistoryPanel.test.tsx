// SPDX-License-Identifier: GPL-3.0-only
// The notification history panel: the newest-first list, the click that
// opens the message through the existing openMessage intent (the exact
// payload) and closes, the per-entry dismiss, the clear all, the empty
// state, and the Escape close. A mid-test store mutation under fake timers
// rides act() so the scheduler re-renders.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { NotificationHistoryPanel } from './NotificationHistoryPanel';
import {
  notificationEntryId,
  useNotificationHistoryStore,
  type NotificationHistoryEntry,
} from '@/stores/notificationHistoryStore';
import { useAccountStore } from '@/stores/accountStore';

/** One recorded entry, minted around `key`. */
function entry(key: string, over: Partial<NotificationHistoryEntry> = {}): NotificationHistoryEntry {
  return {
    id: notificationEntryId('work', key),
    account: 'work',
    key,
    sender: `Sender ${key}`,
    subject: `Subject ${key}`,
    preview: `Preview ${key}`,
    at: Date.parse('2026-11-12T10:00:00Z'),
    ...over,
  };
}

/** Seeds the history with `entries`, newest first as the store holds them. */
function seed(entries: NotificationHistoryEntry[]): void {
  useNotificationHistoryStore.setState({ entries });
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-11-12T12:00:00Z'));
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  localStorage.clear();
  useNotificationHistoryStore.setState({ entries: [] });
  useAccountStore.setState({
    accounts: [{ accountId: 'work', displayName: 'Work', address: 'ada@work.test' }],
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('the notification history panel', () => {
  it('lists the entries newest first with sender, subject, time, and account', () => {
    seed([entry('b'), entry('a')]);
    render(<NotificationHistoryPanel onClose={() => undefined} />);

    const rows = screen.getAllByTestId(/^notification-entry-/);
    expect(rows.map((row) => row.dataset.testid)).toEqual([
      `notification-entry-${notificationEntryId('work', 'b')}`,
      `notification-entry-${notificationEntryId('work', 'a')}`,
    ]);
    expect(rows[0]?.textContent).toContain('Sender b');
    expect(rows[0]?.textContent).toContain('Subject b');
    expect(rows[0]?.textContent).toContain('Work');
    expect(rows[0]?.textContent).toContain('10:00');
  });

  it('opens the message through openMessage with the exact payload and closes', () => {
    const onClose = vi.fn();
    seed([entry('a')]);
    render(<NotificationHistoryPanel onClose={onClose} />);

    fireEvent.click(screen.getByRole('button', { name: 'Open: Subject a' }));

    expect(ipc.dispatch).toHaveBeenCalledWith({
      open_message: { message: { account: 'work', key: 'a' } },
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('dismisses one entry without opening it', () => {
    seed([entry('b'), entry('a')]);
    render(<NotificationHistoryPanel onClose={() => undefined} />);

    fireEvent.click(screen.getByRole('button', { name: 'Dismiss: Subject a' }));

    expect(
      screen.queryByTestId(`notification-entry-${notificationEntryId('work', 'a')}`),
    ).toBeNull();
    expect(
      screen.getByTestId(`notification-entry-${notificationEntryId('work', 'b')}`),
    ).toBeTruthy();
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('clears the whole history', () => {
    seed([entry('b'), entry('a')]);
    render(<NotificationHistoryPanel onClose={() => undefined} />);

    fireEvent.click(screen.getByRole('button', { name: 'Clear all' }));

    expect(screen.getByText('No notifications yet.')).toBeTruthy();
    expect(useNotificationHistoryStore.getState().entries).toEqual([]);
  });

  it('reads the empty state when nothing is recorded', () => {
    render(<NotificationHistoryPanel onClose={() => undefined} />);
    expect(screen.getByText('No notifications yet.')).toBeTruthy();
    expect(
      (screen.getByRole('button', { name: 'Clear all' }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it('closes on Escape', () => {
    const onClose = vi.fn();
    seed([entry('a')]);
    render(<NotificationHistoryPanel onClose={onClose} />);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('shows a fallback line when neither sender nor subject is present', () => {
    seed([entry('a', { sender: undefined, subject: undefined, preview: undefined })]);
    render(<NotificationHistoryPanel onClose={() => undefined} />);
    expect(
      screen.getByTestId(`notification-entry-${notificationEntryId('work', 'a')}`).textContent,
    ).toContain('New message');
  });

  it('re-renders when a batch lands while the panel is open', () => {
    render(<NotificationHistoryPanel onClose={() => undefined} />);
    expect(screen.getByText('No notifications yet.')).toBeTruthy();

    act(() => {
      useNotificationHistoryStore.getState().record({
        messages: [
          {
            account: 'work',
            key: 'live',
            from: { name: 'Live', email: 'live@example.test' },
            subject: 'Live arrival',
            preview: null,
          },
        ],
        overflow: 0,
      });
    });

    const live = screen.getByTestId(`notification-entry-${notificationEntryId('work', 'live')}`);
    expect(live.textContent).toContain('Live arrival');
  });
});