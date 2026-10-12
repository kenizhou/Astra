// SPDX-License-Identifier: GPL-3.0-only
// The reading header: subject, the sender avatar beside the name, and the
// full UTC stamp the pane draws from the snapshot, with the bare address
// standing in for an empty sender name and the standard placeholder for an
// empty subject.
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ peopleAvatar: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  peopleAvatar: ipc.peopleAvatar,
}));

import { ReadingHeader, formatReadingDate } from './ReadingHeader';

beforeEach(() => {
  ipc.peopleAvatar.mockReset();
  // Every header with a sender resolves an avatar; a test that cares
  // overrides the answer.
  ipc.peopleAvatar.mockResolvedValue({ initials: '', colour: '' });
});

afterEach(cleanup);

describe('the reading header', () => {
  it('draws subject, sender, and the full date', () => {
    render(
      <ReadingHeader
        subject="Launch"
        fromName="Ada Lovelace"
        fromAddress="ada@example.test"
        sentAt="2026-10-09T08:30:00Z"
      />,
    );

    expect(screen.getByText('Launch')).toBeTruthy();
    expect(screen.getByText('Ada Lovelace <ada@example.test>')).toBeTruthy();
    expect(screen.getByText('9 Oct 2026, 08:30')).toBeTruthy();
  });

  it('shows the bare address when the name is empty', () => {
    render(
      <ReadingHeader
        subject="Digest"
        fromName=""
        fromAddress="notifications@example.test"
        sentAt="2026-10-09T08:30:00Z"
      />,
    );

    expect(screen.getByText('notifications@example.test')).toBeTruthy();
  });

  it('draws the placeholder subject and drops an empty sender line', () => {
    render(
      <ReadingHeader subject="" fromName="" fromAddress="" sentAt="2026-10-09T08:30:00Z" />,
    );

    expect(screen.getByText('(no subject)')).toBeTruthy();
    expect(screen.queryByText('<>')).toBeNull();
  });

  it('mounts the sender avatar beside the name, resolved by the address', async () => {
    ipc.peopleAvatar.mockResolvedValue({ initials: 'AL', colour: '#4DB6AC' });
    render(
      <ReadingHeader
        subject="Launch"
        fromName="Ada Lovelace"
        fromAddress="ada@example.test"
        sentAt="2026-10-09T08:30:00Z"
      />,
    );

    expect(ipc.peopleAvatar).toHaveBeenCalledWith('ada@example.test');
    const avatar = await waitFor(() => {
      const badge = screen.getByRole('img', { name: 'Avatar' });
      expect(badge.textContent).toBe('AL');
      return badge;
    });
    const sender = screen.getByText('Ada Lovelace <ada@example.test>');
    expect(avatar.compareDocumentPosition(sender) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('draws no avatar when there is no sender', () => {
    render(
      <ReadingHeader subject="" fromName="" fromAddress="" sentAt="2026-10-09T08:30:00Z" />,
    );

    expect(screen.queryByRole('img', { name: 'Avatar' })).toBeNull();
    expect(ipc.peopleAvatar).not.toHaveBeenCalled();
  });

  it('formats a whole-year stamp and renders nothing for an unknown one', () => {
    expect(formatReadingDate('2026-10-09T08:30:00Z')).toBe('9 Oct 2026, 08:30');
    expect(formatReadingDate('2021-01-02T03:04:00Z')).toBe('2 Jan 2021, 03:04');
    expect(formatReadingDate('')).toBe('');
    expect(formatReadingDate('not a date')).toBe('');
  });
});
