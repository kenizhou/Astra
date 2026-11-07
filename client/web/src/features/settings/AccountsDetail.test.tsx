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
        row({
          account: 'gmail',
          address: 'ada@gmail.example',
          kind: 'google',
          legs: { calendar: true, contacts: false },
        }),
      ]),
    );
    render(<AccountsDetail />);

    expect(screen.getByText('ada@example.test')).toBeTruthy();
    expect(screen.getByText('me@home.test')).toBeTruthy();
    expect(screen.getByText('ada@gmail.example')).toBeTruthy();
    const work = screen.getByRole('button', { name: 'Open ada@example.test' });
    expect(work.textContent).toContain('IMAP + SMTP');
    expect(work.textContent).toContain('Used for calendar');
    const home = screen.getByRole('button', { name: 'Open me@home.test' });
    expect(home.textContent).toContain('JMAP');
    expect(home.textContent).toContain('Used for contacts');
    const gmail = screen.getByRole('button', { name: 'Open ada@gmail.example' });
    expect(gmail.textContent).toContain('Google');
    expect(gmail.textContent).toContain('Used for calendar');
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

describe('the per-account sync section', () => {
  it('opens on the account page, stating how far back mail syncs', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    expect(screen.getByLabelText('How far back mail syncs')).toBeTruthy();
    expect(screen.getByLabelText('Offline message bodies')).toBeTruthy();
    expect(screen.getByLabelText('Check for new mail')).toBeTruthy();
  });

  it('reflects the standing effective values, never the click that started the write', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([
        row({
          sync: { depth_months: 12, body_cap: 'off', poll_minutes: 90 },
        }),
      ]),
    );
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    expect((screen.getByLabelText('How far back mail syncs') as HTMLSelectElement).value).toBe(
      '12',
    );
    expect((screen.getByLabelText('Offline message bodies') as HTMLSelectElement).value).toBe(
      'off',
    );
    expect((screen.getByLabelText('Check for new mail') as HTMLSelectElement).value).toBe('90');

    // A change dispatches and the control keeps the standing truth until the
    // republished snapshot lands.
    fireEvent.change(screen.getByLabelText('How far back mail syncs'), { target: { value: '3' } });
    expect((screen.getByLabelText('How far back mail syncs') as HTMLSelectElement).value).toBe(
      '12',
    );
  });

  it('answers the product defaults when the row carries no sync block', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    expect((screen.getByLabelText('How far back mail syncs') as HTMLSelectElement).value).toBe('3');
    expect((screen.getByLabelText('Offline message bodies') as HTMLSelectElement).value).toBe(
      'default',
    );
    expect((screen.getByLabelText('Check for new mail') as HTMLSelectElement).value).toBe('30');
  });

  it('writes the depth alone, crossing no key for the cap or the cadence', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    fireEvent.change(screen.getByLabelText('How far back mail syncs'), { target: { value: '12' } });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_sync_settings":{"account":"work","depth_months":12}}}',
    );
  });

  it('writes the whole mailbox as a zero depth', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    fireEvent.change(screen.getByLabelText('How far back mail syncs'), { target: { value: '0' } });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_sync_settings":{"account":"work","depth_months":0}}}',
    );
  });

  it('writes the body cap in every state, the words honest to the per-message rule', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    const cap = screen.getByLabelText('Offline message bodies');
    fireEvent.change(cap, { target: { value: 'off' } });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_sync_settings":{"account":"work","body_cap":"off"}}}',
    );

    fireEvent.change(cap, { target: { value: 'everything' } });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"settings":{"set_sync_settings":{"account":"work","body_cap":"everything"}}}',
    );

    fireEvent.change(cap, { target: { value: 'default' } });
    expect(JSON.stringify(ipc.dispatch.mock.calls[2]?.[0])).toBe(
      '{"settings":{"set_sync_settings":{"account":"work","body_cap":{"bytes":{"octets":268435456}}}}}',
    );
  });

  it('writes a custom per-message figure stated in mebibytes', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    fireEvent.change(screen.getByLabelText('Offline message bodies'), {
      target: { value: 'custom' },
    });
    expect(ipc.dispatch).not.toHaveBeenCalled();

    const figure = screen.getByLabelText('Maximum per message (MiB)');
    fireEvent.change(figure, { target: { value: '5' } });
    fireEvent.blur(figure);

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_sync_settings":{"account":"work","body_cap":{"bytes":{"octets":5242880}}}}}',
    );
  });

  it('seeds the custom figure from a standing custom cap', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([
        row({
          sync: { depth_months: 3, body_cap: { bytes: { octets: 1048576 } }, poll_minutes: 30 },
        }),
      ]),
    );
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    expect((screen.getByLabelText('Offline message bodies') as HTMLSelectElement).value).toBe(
      'custom',
    );
    expect((screen.getByLabelText('Maximum per message (MiB)') as HTMLInputElement).value).toBe(
      '1',
    );
  });

  it('writes the cadence in minutes', () => {
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    fireEvent.change(screen.getByLabelText('Check for new mail'), { target: { value: '90' } });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_sync_settings":{"account":"work","poll_minutes":90}}}',
    );
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

describe('the outgoing name', () => {
  it('carries the display name beside the address on the row', () => {
    useSettingsStore
      .getState()
      .applySnapshot(snapshot([row({ display_name: 'Ada Lovelace' })]));
    render(<AccountsDetail />);

    const open = screen.getByRole('button', { name: 'Open ada@example.test' });
    expect(open.textContent).toContain('Ada Lovelace');
    expect(open.textContent).toContain('ada@example.test');
  });

  it('asks once on the page for an account whose name is unanswered', () => {
    useSettingsStore
      .getState()
      .applySnapshot(snapshot([row({ needs_sender_name: true })]));
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    const ask = screen.getByRole('group', { name: /outgoing name/i });
    expect(ask.textContent).toContain('ada@example.test');
    fireEvent.change(screen.getByLabelText(/what name/i), {
      target: { value: 'Ada Lovelace' },
    });
    fireEvent.click(screen.getByRole('button', { name: /set the name/i }));

    expect(ipc.dispatch).toHaveBeenCalledWith({
      settings: { set_sender_name: { account: 'work', name: 'Ada Lovelace' } },
    });

    // The answered ask leaves with the snapshot that carries the answer.
    act(() => {
      useSettingsStore.getState().applySnapshot(snapshot([row()]));
    });
    expect(screen.queryByLabelText(/what name/i)).toBeNull();
  });

  it('the ask offers the address alone as a real answer', () => {
    useSettingsStore
      .getState()
      .applySnapshot(snapshot([row({ needs_sender_name: true })]));
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    fireEvent.click(screen.getByRole('button', { name: /use the address alone/i }));

    expect(ipc.dispatch).toHaveBeenCalledWith({
      settings: { set_sender_name: { account: 'work', name: '' } },
    });
  });

  it('a writable card edits the name, shows the refusal, and clears it when the push lands', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([row({ display_name: 'Ada Lovelace', sender_name_control: 'writable' })]),
    );
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    const field = screen.getByLabelText(/the name on the from line/i);
    expect((field as HTMLInputElement).value).toBe('Ada Lovelace');
    fireEvent.change(field, { target: { value: 'Augusta Ada King' } });
    fireEvent.click(screen.getByRole('button', { name: /save the name/i }));
    expect(ipc.dispatch).toHaveBeenCalledWith({
      settings: { set_sender_name: { account: 'work', name: 'Augusta Ada King' } },
    });

    act(() => {
      useSettingsStore.getState().applySnapshot(
        snapshot([
          row({
            display_name: 'Augusta Ada King',
            sender_name_control: 'writable',
            sender_name_error: 'the server refused the write',
          }),
        ]),
      );
    });
    expect(screen.getByRole('alert').textContent).toContain('the server refused the write');

    act(() => {
      useSettingsStore.getState().applySnapshot(
        snapshot([row({ display_name: 'Augusta Ada King', sender_name_control: 'writable' })]),
      );
    });
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('a read-only card shows the name with its reason and offers no field', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([row({ display_name: 'Ada Lovelace', sender_name_control: 'read_only' })]),
    );
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    const card = screen.getByRole('group', { name: /outgoing name/i });
    expect(card.textContent).toContain('Ada Lovelace');
    expect(card.textContent).toMatch(/cannot be changed here/i);
    expect(screen.queryByLabelText(/the name on the from line/i)).toBeNull();
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('a local card edits the name and says where it lives', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([row({ display_name: 'Ada Lovelace', sender_name_control: 'local' })]),
    );
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    const card = screen.getByRole('group', { name: /outgoing name/i });
    expect(card.textContent).toMatch(/kept in Astra Mail/i);
    fireEvent.change(screen.getByLabelText(/the name on the from line/i), {
      target: { value: 'Grace Hopper' },
    });
    fireEvent.click(screen.getByRole('button', { name: /save the name/i }));
    expect(ipc.dispatch).toHaveBeenCalledWith({
      settings: { set_sender_name: { account: 'work', name: 'Grace Hopper' } },
    });
  });

  it('the edit field seeds empty where the account sends as the bare address', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot([row({ display_name: 'ada@example.test', sender_name_control: 'local' })]),
    );
    render(<AccountsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open ada@example.test' }));

    const field = screen.getByLabelText(/the name on the from line/i) as HTMLInputElement;
    expect(field.value).toBe('');
  });
});
