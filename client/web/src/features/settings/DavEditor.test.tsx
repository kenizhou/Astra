// SPDX-License-Identifier: GPL-3.0-only
// The dav section editor: the masked row's values seed the form, the
// password stays blank with a placeholder that says blank keeps the stored
// credential, the save dispatches SetDav stating only the fields the form
// changed, a failed save keeps the form's values for correction with the
// failure sentence under them, and a clean save closes the editor.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { DavEditor } from './DavEditor';
import { useSettingsStore } from './settingsStore';
import type { AccountSettingsRow, SettingsSnapshot } from '@/services/ipc';

const onClose = vi.fn();

function row(over: Partial<AccountSettingsRow> = {}): AccountSettingsRow {
  return {
    account: 'work',
    address: 'ada@example.test',
    kind: 'imap_smtp',
    legs: { calendar: true, contacts: false },
    dav: {
      caldav_url: 'https://dav.example.test',
      username: 'ada@example.test',
      dav_password_set: true,
    },
    ...over,
  };
}

function apply(rowOver: Partial<AccountSettingsRow> = {}): void {
  const snapshot: SettingsSnapshot = {
    settings: {
      theme: 'follow_host',
      twenty_four_clock: false,
      zone: 'Etc/UTC',
      first_day_of_week: 'mo',
      horizon_days: 42,
      quote_below: true,
      view_mode: 'flat',
    },
    accounts: [row(rowOver)],
    about: { version: '0.1.0' },
  };
  useSettingsStore.getState().applySnapshot(snapshot);
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  onClose.mockReset();
  apply();
});

afterEach(cleanup);

describe('the dav editor seed', () => {
  it('seeds the masked row values and never a password', () => {
    render(<DavEditor account="work" onClose={onClose} />);

    expect((screen.getByLabelText('CalDAV URL') as HTMLInputElement).value).toBe(
      'https://dav.example.test',
    );
    expect((screen.getByLabelText('Username') as HTMLInputElement).value).toBe(
      'ada@example.test',
    );
    expect((screen.getByLabelText('Password') as HTMLInputElement).value).toBe('');
    expect(screen.getByLabelText('Password').getAttribute('placeholder')).toContain(
      'Leave blank to keep',
    );
  });

  it('offers a plain password field when no credential is stored', () => {
    apply({ dav: { username: 'ada@example.test', dav_password_set: false } });
    render(<DavEditor account="work" onClose={onClose} />);

    expect(screen.getByLabelText('Password').getAttribute('placeholder')).not.toContain(
      'Leave blank to keep',
    );
  });
});

describe('the dav editor save', () => {
  it('dispatches SetDav stating only the changed fields plus a typed password', () => {
    render(<DavEditor account="work" onClose={onClose} />);
    fireEvent.change(screen.getByLabelText('Username'), { target: { value: 'ada2@example.test' } });
    fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'new-secret' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save dav section' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_dav":{"account":"work","section":{"username":"ada2@example.test","password":"new-secret"}}}}',
    );
  });

  it('crosses no password key when the field stays blank', () => {
    render(<DavEditor account="work" onClose={onClose} />);
    fireEvent.change(screen.getByLabelText('Username'), { target: { value: 'ada2@example.test' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save dav section' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toContain('"username"');
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).not.toContain('password');
  });

  it('offers no save while nothing changed', () => {
    render(<DavEditor account="work" onClose={onClose} />);

    expect((screen.getByRole('button', { name: 'Save dav section' }) as HTMLButtonElement).disabled)
      .toBe(true);
  });

  it('keeps the form values and renders the sentence when the save refuses', () => {
    render(<DavEditor account="work" onClose={onClose} />);
    fireEvent.change(screen.getByLabelText('Username'), { target: { value: 'ada2@example.test' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save dav section' }));

    act(() => {
      apply({ dav_error: 'the re-dial could not reach the origin' });
    });

    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('alert').textContent).toContain(
      'the re-dial could not reach the origin',
    );
    expect((screen.getByLabelText('Username') as HTMLInputElement).value).toBe(
      'ada2@example.test',
    );
  });

  it('closes once the publish after the save carries no failure', () => {
    render(<DavEditor account="work" onClose={onClose} />);
    fireEvent.change(screen.getByLabelText('Username'), { target: { value: 'ada2@example.test' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save dav section' }));

    act(() => {
      apply();
    });

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('clears the section through the null arm', () => {
    render(<DavEditor account="work" onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Clear dav section' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_dav":{"account":"work","section":null}}}',
    );
  });
});
