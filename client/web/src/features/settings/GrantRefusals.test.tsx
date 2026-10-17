// SPDX-License-Identifier: GPL-3.0-only
// The grant refusal surfaces on the accounts page: the reconnect banner
// for a Microsoft account whose calendar permission went missing (one
// tap re-runs the sign-in with the account's address as the login hint)
// and the expired-sign-in row for a dead grant (the sentence names the
// sign-in, never a server outage; the action the same re-grant). A
// healthy account renders neither.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), oauthBegin: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  oauthBegin: ipc.oauthBegin,
}));

import { AccountsDetail } from './AccountsDetail';
import { ReconnectBanner } from './GrantRefusals';

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

function snapshot(rows: AccountSettingsRow[]): SettingsSnapshot {
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
  };
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.oauthBegin.mockReset();
  ipc.oauthBegin.mockResolvedValue({ provider: 'microsoft' });
  useSettingsStore.getState().applySnapshot(snapshot([row()]));
});

afterEach(cleanup);

describe('the reconnect banner', () => {
  it('renders for a microsoft account whose calendar is withheld', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([
        row({
          account: 'ms',
          address: 'ada@outlook.test',
          kind: 'microsoft',
          grant: 'calendar_withheld',
        }),
      ]),
    );
    render(<AccountsDetail />);

    const banner = screen.getByRole('status', { name: 'Calendar permission missing' });
    expect(banner.textContent).toContain('ada@outlook.test');
    expect(banner.textContent).toContain('calendar');
  });

  it('never renders for a healthy account, whatever its kind', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([
        row(),
        row({ account: 'ms', address: 'ada@outlook.test', kind: 'microsoft' }),
      ]),
    );
    render(<AccountsDetail />);

    expect(screen.queryByRole('status', { name: 'Calendar permission missing' })).toBeNull();
    expect(screen.queryByText(/sign-in expired/i)).toBeNull();
  });

  it('never renders for a non-microsoft account carrying the state', () => {
    // The trigger is Microsoft-only: the same word on an imap row is no
    // surface at all.
    useSettingsStore
      .getState()
      .applySnapshot(snapshot([row({ grant: 'calendar_withheld' })]));
    render(<AccountsDetail />);

    expect(screen.queryByRole('status', { name: 'Calendar permission missing' })).toBeNull();
  });

  it('begins the re-grant with the address as the login hint on one tap', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([
        row({
          account: 'ms',
          address: 'ada@outlook.test',
          kind: 'microsoft',
          grant: 'calendar_withheld',
        }),
      ]),
    );
    render(<AccountsDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Reconnect the calendar for ada@outlook.test' }));

    expect(ipc.oauthBegin).toHaveBeenCalledWith('microsoft', 'ada@outlook.test');
  });
});

describe('the expired-sign-in row', () => {
  it('states the sign-in expired and dispatches the same re-grant', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([
        row({
          account: 'ms',
          address: 'ada@outlook.test',
          kind: 'microsoft',
          grant: 'sign_in_expired',
        }),
      ]),
    );
    render(<AccountsDetail />);

    const sentence = screen.getByRole('status', { name: 'Sign-in expired' });
    expect(sentence.textContent).toContain('ada@outlook.test');
    expect(sentence.textContent).toMatch(/sign-in expired or was withdrawn/);
    expect(screen.queryByText(/unreachable|could not be reached|outage/i)).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Sign in again as ada@outlook.test' }));

    expect(ipc.oauthBegin).toHaveBeenCalledWith('microsoft', 'ada@outlook.test');
  });

  it('begins the re-grant for the expired account own family, google for a google row', () => {
    // The expired row is the cross-family surface: its action targets
    // the family the grant belongs to, never the Microsoft flow alone.
    useSettingsStore.getState().applySnapshot(
      snapshot([
        row({
          account: 'goog',
          address: 'ada@gmail.example',
          kind: 'google',
          grant: 'sign_in_expired',
        }),
      ]),
    );
    render(<AccountsDetail />);

    const sentence = screen.getByRole('status', { name: 'Sign-in expired' });
    expect(sentence.textContent).toContain('ada@gmail.example');

    fireEvent.click(screen.getByRole('button', { name: 'Sign in again as ada@gmail.example' }));

    expect(ipc.oauthBegin).toHaveBeenCalledWith('google', 'ada@gmail.example');
  });

  it('a jmap expired row names the credential and never begins a microsoft flow', () => {
    // The JMAP family has no integrated provider flow to re-grant with:
    // the row states the credential was refused and points at
    // reconnecting the account, and no Sign in again button renders.
    useSettingsStore.getState().applySnapshot(
      snapshot([
        row({
          account: 'jmap',
          address: 'ada@example.test',
          kind: 'jmap',
          grant: 'sign_in_expired',
        }),
      ]),
    );
    render(<AccountsDetail />);

    const sentence = screen.getByRole('status', { name: 'Sign-in expired' });
    expect(sentence.textContent).toContain('ada@example.test');
    expect(sentence.textContent).toMatch(/credential was refused/);
    expect(
      screen.queryByRole('button', { name: /Sign in again/ }),
      'no microsoft or google flow begins for a jmap row'
    ).toBeNull();
    expect(screen.queryByRole('button', { name: 'Reconnect' })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    expect(screen.getByRole('button', { name: 'Remove account' })).toBeTruthy();
  });

  it('keeps the expired account reachable for removal through its detail page', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([
        row({
          account: 'ms',
          address: 'ada@outlook.test',
          kind: 'microsoft',
          grant: 'sign_in_expired',
        }),
      ]),
    );
    render(<AccountsDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Open ada@outlook.test' }));

    expect(screen.getByRole('button', { name: 'Remove account' })).toBeTruthy();
  });
});

  it('renders a refused begin beside the banner instead of silently doing nothing', async () => {
    ipc.oauthBegin.mockRejectedValue(new Error('a sign-in is already in flight'));
    render(
      <ReconnectBanner
        rows={[
          row({
            account: 'msft',
            address: 'ada@outlook.test',
            kind: 'microsoft',
            grant: 'calendar_withheld',
          }),
        ]}
      />,
    );
    fireEvent.click(
      screen.getByRole('button', { name: 'Reconnect the calendar for ada@outlook.test' }),
    );
    // The refusal's sentence surfaces beside the surface that asked: a
    // wedged one-tap remedy is never a silent no-op.
    expect(await screen.findByRole('alert')).toBeTruthy();
  });
