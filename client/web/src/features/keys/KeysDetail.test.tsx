// SPDX-License-Identifier: GPL-3.0-only
// The key import detail: the account's keys list on mount, the paste and
// file affordances handing the bytes to the host, the passphrase carried
// in-process and cleared the moment it is spent, and a refusal named. The
// submission is mocked at the feature service; the settings store carries
// the account rows.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const crypto = vi.hoisted(() => ({
  importKey: vi.fn(),
  listKeys: vi.fn(),
  listCollected: vi.fn(),
  promoteCollected: vi.fn(),
  rejectCollected: vi.fn(),
}));

vi.mock('@/services/crypto', () => crypto);

import { KeysDetail } from './KeysDetail';
import { useSettingsStore } from '@/features/settings/settingsStore';
import type { AccountSettingsRow, SettingsSnapshot } from '@/services/ipc';
import type { CryptoKeyRow } from '@/services/crypto';

function row(over: Partial<CryptoKeyRow> = {}): CryptoKeyRow {
  return {
    id: 'row-1',
    accountId: 'work',
    standard: 'openpgp',
    keyType: 'public',
    email: 'peer@elsewhere.local',
    fingerprint: 'fpListed',
    origin: 'imported',
    isDefaultSign: false,
    isDefaultEncrypt: false,
    createdAt: '1700000000',
    hasPrivate: false,
    ...over,
  };
}

function accountRow(): AccountSettingsRow {
  return {
    account: 'work',
    address: 'ada@example.test',
    kind: 'imap_smtp',
    legs: { calendar: true, contacts: false },
  };
}

function snapshot(): SettingsSnapshot {
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
    accounts: [accountRow()],
    about: { version: '0.1.0' },
  };
}

beforeEach(() => {
  crypto.importKey.mockReset();
  crypto.listKeys.mockReset();
  crypto.listCollected.mockReset();
  crypto.promoteCollected.mockReset();
  crypto.rejectCollected.mockReset();
  crypto.listKeys.mockResolvedValue([row()]);
  crypto.listCollected.mockResolvedValue([]);
  useSettingsStore.getState().applySnapshot(snapshot());
});

afterEach(cleanup);

describe('the key import detail', () => {
  it("lists the account's keys on mount", async () => {
    render(<KeysDetail />);
    expect(await screen.findByText('fpListed')).toBeTruthy();
    expect(crypto.listKeys).toHaveBeenCalledWith('work');
  });

  it('imports pasted material and refreshes the list', async () => {
    crypto.importKey.mockResolvedValue(row({ fingerprint: 'fpPasted' }));
    render(<KeysDetail />);
    await screen.findByText('fpListed');

    const text = '-----BEGIN PGP PUBLIC KEY BLOCK-----\nabc\n-----END PGP PUBLIC KEY BLOCK-----';
    fireEvent.change(screen.getByLabelText('Paste a key or certificate'), {
      target: { value: text },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Import pasted key' }));

    await waitFor(() => expect(crypto.importKey).toHaveBeenCalledTimes(1));
    const [account, bytes, passphrase] = crypto.importKey.mock.calls[0]!;
    expect(account).toBe('work');
    expect(Array.from(bytes as Uint8Array)).toEqual(Array.from(new TextEncoder().encode(text)));
    expect(passphrase).toBeUndefined();
  });

  it('carries the passphrase in-process and clears it once spent', async () => {
    crypto.importKey.mockResolvedValue(row({ fingerprint: 'fpSecret', hasPrivate: true }));
    render(<KeysDetail />);
    await screen.findByText('fpListed');

    fireEvent.change(screen.getByLabelText('Paste a key or certificate'), {
      target: { value: 'the secret key' },
    });
    const passphraseInput = screen.getByLabelText('Passphrase') as HTMLInputElement;
    fireEvent.change(passphraseInput, { target: { value: 'hunter2' } });
    fireEvent.click(screen.getByRole('button', { name: 'Import pasted key' }));

    await waitFor(() => expect(crypto.importKey).toHaveBeenCalledTimes(1));
    expect(crypto.importKey.mock.calls[0]![2]).toBe('hunter2');
    // The passphrase is held only in memory: the field is empty once spent.
    await waitFor(() => expect(passphraseInput.value).toBe(''));
  });

  it("imports a chosen file's bytes", async () => {
    crypto.importKey.mockResolvedValue(row({ fingerprint: 'fpFile' }));
    render(<KeysDetail />);
    await screen.findByText('fpListed');

    const file = new File([new Uint8Array([1, 2, 3, 4])], 'key.asc', {
      type: 'application/pgp-keys',
    });
    fireEvent.change(screen.getByLabelText('Import a key file'), {
      target: { files: [file] },
    });

    await waitFor(() => expect(crypto.importKey).toHaveBeenCalledTimes(1));
    const [account, bytes] = crypto.importKey.mock.calls[0]!;
    expect(account).toBe('work');
    expect(Array.from(bytes as Uint8Array)).toEqual([1, 2, 3, 4]);
  });

  it('names a refusal', async () => {
    crypto.importKey.mockRejectedValue('the key import failed: wrong passphrase');
    render(<KeysDetail />);
    await screen.findByText('fpListed');

    fireEvent.change(screen.getByLabelText('Paste a key or certificate'), {
      target: { value: 'not a key' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Import pasted key' }));

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('the key import failed: wrong passphrase');
  });

  it('joins the collected-key review for the chosen account', async () => {
    crypto.listCollected.mockResolvedValue([
      {
        id: 9,
        peerEmail: 'peer@elsewhere.local',
        standard: 'openpgp',
        fingerprint: 'fpCollected',
        source: 'autocrypt',
        seenAt: '1700000000',
      },
    ]);
    render(<KeysDetail />);
    expect(await screen.findByText('fpCollected')).toBeTruthy();
    expect(crypto.listCollected).toHaveBeenCalledWith('work');
  });
});