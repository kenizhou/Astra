// SPDX-License-Identifier: GPL-3.0-only
// The blocked-senders management section: it lists the blocklist the
// settings block carries, each entry with an Unblock that dispatches the
// unblock intent, and an honest empty state when none stands. The list reads
// the settings wire, never a local copy.
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { BlockedSenders } from './BlockedSenders';
import { useSettingsStore } from './settingsStore';

/** Seeds the settings block's blocklist, as a pulled snapshot does. */
function seed(blocked: string[]): void {
  useSettingsStore.getState().applySnapshot({
    settings: {
      theme: 'follow_host',
      twenty_four_clock: false,
      zone: 'Etc/UTC',
      first_day_of_week: 'mo',
      horizon_days: 42,
      quote_below: true,
      view_mode: 'flat',
      blocked_senders: blocked,
    },
    accounts: [],
    about: { version: '0.1.0' },
  });
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  seed([]);
});

afterEach(() => {
  cleanup();
});

describe('the blocked senders section', () => {
  it('lists the blocked senders the settings block carries', () => {
    seed(['ada@example.test', 'bob@example.test']);
    render(<BlockedSenders />);
    const section = screen.getByRole('region', { name: 'Blocked senders' });
    expect(within(section).getByText('ada@example.test')).toBeTruthy();
    expect(within(section).getByText('bob@example.test')).toBeTruthy();
  });

  it('unblocks one sender through the settings intent', () => {
    seed(['ada@example.test', 'bob@example.test']);
    render(<BlockedSenders />);
    fireEvent.click(screen.getByRole('button', { name: 'Unblock ada@example.test' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"unblock_sender":{"address":"ada@example.test"}}}',
    );
  });

  it('shows an empty state when no sender is blocked', () => {
    seed([]);
    render(<BlockedSenders />);
    const section = screen.getByRole('region', { name: 'Blocked senders' });
    expect(within(section).getByText('No blocked senders.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Unblock/ })).toBeNull();
  });

  it('reads the wire, not a local copy: a fresh snapshot repaints the list', () => {
    seed(['ada@example.test']);
    render(<BlockedSenders />);
    expect(screen.getByText('ada@example.test')).toBeTruthy();

    act(() => {
      seed([]);
    });
    expect(screen.queryByText('ada@example.test')).toBeNull();
    expect(screen.getByText('No blocked senders.')).toBeTruthy();
  });
});