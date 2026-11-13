// SPDX-License-Identifier: GPL-3.0-only
// The collected-key review component (SP4d-2 D2): the staged keys list on
// mount with the fingerprint, peer email, source, and standard, the Accept
// (unverified) and Accept as verified choices handing the verified flag to the
// host, the Reject handing the staging id, and a refusal named. The fingerprint
// is shown so a person can verify it out of band before Accept as verified. The
// submission is mocked at the feature service.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const crypto = vi.hoisted(() => ({
  listCollected: vi.fn(),
  promoteCollected: vi.fn(),
  rejectCollected: vi.fn(),
}));

vi.mock('@/services/crypto', () => crypto);

import { CollectedKeysReview } from './CollectedKeysReview';
import type { CollectedKeyRow } from '@/services/crypto';

function staged(over: Partial<CollectedKeyRow> = {}): CollectedKeyRow {
  return {
    id: 7,
    accountId: 'work',
    peerEmail: 'peer@elsewhere.local',
    standard: 'openpgp',
    fingerprint: 'fpStaged',
    source: 'autocrypt',
    seenAt: '1700000000',
    ...over,
  };
}

beforeEach(() => {
  crypto.listCollected.mockReset();
  crypto.promoteCollected.mockReset();
  crypto.rejectCollected.mockReset();
  crypto.listCollected.mockResolvedValue([staged()]);
  crypto.promoteCollected.mockResolvedValue({ fingerprint: 'fpStaged' });
  crypto.rejectCollected.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe('the collected-key review', () => {
  it("lists the account's staged keys on mount with the review fields", async () => {
    render(<CollectedKeysReview account="work" />);
    expect(await screen.findByText('fpStaged')).toBeTruthy();
    expect(screen.getByText('peer@elsewhere.local')).toBeTruthy();
    expect(screen.getByText(/autocrypt/)).toBeTruthy();
    expect(screen.getByText(/openpgp/)).toBeTruthy();
    expect(crypto.listCollected).toHaveBeenCalledWith('work');
  });

  it('shows the empty state when nothing is staged', async () => {
    crypto.listCollected.mockResolvedValue([]);
    render(<CollectedKeysReview account="work" />);
    expect(await screen.findByText('No collected keys to review.')).toBeTruthy();
  });

  it('accepts a key as unverified and refreshes the list', async () => {
    render(<CollectedKeysReview account="work" />);
    await screen.findByText('fpStaged');

    fireEvent.click(screen.getByRole('button', { name: 'Accept fpStaged' }));

    await waitFor(() => expect(crypto.promoteCollected).toHaveBeenCalledTimes(1));
    expect(crypto.promoteCollected).toHaveBeenCalledWith(
      'work',
      'openpgp',
      'fpStaged',
      'peer@elsewhere.local',
      false,
    );
    await waitFor(() => expect(crypto.listCollected).toHaveBeenCalledTimes(2));
  });

  it('accepts a key as verified, handing the verified flag', async () => {
    render(<CollectedKeysReview account="work" />);
    await screen.findByText('fpStaged');

    fireEvent.click(screen.getByRole('button', { name: 'Accept as verified fpStaged' }));

    await waitFor(() => expect(crypto.promoteCollected).toHaveBeenCalledTimes(1));
    expect(crypto.promoteCollected).toHaveBeenCalledWith(
      'work',
      'openpgp',
      'fpStaged',
      'peer@elsewhere.local',
      true,
    );
  });

  it('rejects a key by its staging id and refreshes the list', async () => {
    render(<CollectedKeysReview account="work" />);
    await screen.findByText('fpStaged');

    fireEvent.click(screen.getByRole('button', { name: 'Reject fpStaged' }));

    await waitFor(() => expect(crypto.rejectCollected).toHaveBeenCalledTimes(1));
    expect(crypto.rejectCollected).toHaveBeenCalledWith(
      'work',
      7,
      'openpgp',
      'fpStaged',
      'peer@elsewhere.local',
    );
    await waitFor(() => expect(crypto.listCollected).toHaveBeenCalledTimes(2));
  });

  it('names a refusal', async () => {
    crypto.promoteCollected.mockRejectedValue('the key store refused the review action: no collected key');
    render(<CollectedKeysReview account="work" />);
    await screen.findByText('fpStaged');

    fireEvent.click(screen.getByRole('button', { name: 'Accept fpStaged' }));

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('the key store refused the review action');
  });
});