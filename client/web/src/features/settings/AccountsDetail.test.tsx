// SPDX-License-Identifier: GPL-3.0-only
// The Accounts detail: one row per configured account (address, kind, the
// dialled legs), each row opening its own page carrying the dav editor and
// Remove, the removal confirmed by naming the address with the destructive
// act behind it, and the removal failure's sentence when it lands.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { AccountsDetail } from './AccountsDetail';
import { useSettingsStore } from './settingsStore';
import type { AccountSettingsRow, SettingsSnapshot } from '@/services/ipc';

function row(over: Partial<AccountSettingsRow> = {}): AccountSettingsRow {
  return {
    account: 'work',
    address: 'ada@example.test',
    kind: 'imap_smtp',
    legs: { calendar: true, contacts: false },
    ...over,
  };
}

function snapshot(rows: AccountSettingsRow[], error?: SettingsSnapshot['error']): SettingsSnapshot {
  return {
    settings: {
      theme: 'follow_host',
      twenty_four_clock: false,
      zone: 'Etc/UTC',
      first_day_of_week: 'mo',
      horizon_days: 42,
      quote_below: true,
      view_mode: 'flat',
    },
    accounts: rows,
    about: { version: '0.1.0' },
    ...(error !== undefined ? { error } : {}),
  };
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useSettingsStore.getState().applySnapshot(snapshot([row()]));
});

afterEach(cleanup);

describe('the account rows', () => {
  it('renders one row per account with address, kind, and legs', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([
        row(),
        row({
          account: 'home',
          address: 'me@home.test',
          kind: 'jmap',
          legs: { calendar: false, contacts: true },
        }),
      ]),
    );
    render(<AccountsDetail />);

    expect(screen.getByText('ada@example.test')).toBeTruthy();
    expect(screen.getByText('me@home.test')).toBeTruthy();
    const work = screen.getByRole('button', { name: 'Open ada@example.test' });
    expect(work.textContent).toContain('IMAP + SMTP');
    expect(work.textContent).toContain('Used for calendar');
    const home = screen.getByRole('button', { name: 'Open me@home.test' });
    expect(home.textContent).toContain('JMAP');
    expect(home.textContent).toContain('Used for contacts');
  });

  it('names an account with no dialled legs honestly', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([row({ legs: { calendar: false, contacts: false } })]),
    );
    render(<AccountsDetail />);

    expect(screen.getByText('No calendar or contacts legs yet.')).toBeTruthy();
  });
});

describe('the account page', () => {
  it('opens from a row and carries the dav editor', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    expect(screen.getByLabelText('Username')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Back to accounts' })).toBeTruthy();
  });

  it('falls back to the list when the account leaves the snapshot', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    act(() => {
      useSettingsStore.getState().applySnapshot(snapshot([]));
    });
    expect(screen.queryByLabelText('Username')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Open ada@example.test' })).toBeNull();
  });

  it('renders the refresh failure sentence the row carries', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([row({ refresh_error: 'the diary could not be re-expanded under the new zone' })]),
    );
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    expect(
      screen.getByText('the diary could not be re-expanded under the new zone'),
    ).toBeTruthy();
  });
});

describe('the remove confirmation', () => {
  it('names the address and the destructive act behind it', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));
    fireEvent.click(screen.getByRole('button', { name: 'Remove account' }));

    const confirmation = screen.getByRole('alertdialog');
    expect(confirmation.textContent).toContain('ada@example.test');
    expect(confirmation.textContent).toContain('mail');
    expect(confirmation.textContent).toContain('calendar');
    expect(confirmation.textContent).toContain('contacts');
  });

  it('stays behind its confirm: Cancel returns, Remove dispatches', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));
    fireEvent.click(screen.getByRole('button', { name: 'Remove account' }));

    fireEvent.click(screen.getByRole('button', { name: 'Cancel removal' }));
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(screen.queryByRole('alertdialog')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Remove account' }));
    fireEvent.click(screen.getByRole('button', { name: 'Remove ada@example.test' }));
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"remove_account":{"account":"work"}}}',
    );
  });

  it('renders the removal failure sentence when it lands', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));
    fireEvent.click(screen.getByRole('button', { name: 'Remove account' }));
    fireEvent.click(screen.getByRole('button', { name: 'Remove ada@example.test' }));

    act(() => {
      useSettingsStore.getState().applySnapshot(snapshot([row()], 'remove_failed'));
    });
    expect(screen.getByRole('alert').textContent).toContain('could not be removed');
  });
});
