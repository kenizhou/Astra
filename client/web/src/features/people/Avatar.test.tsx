// SPDX-License-Identifier: GPL-3.0-only
// The avatar: the monogram the row carries renders as initials on its own
// colour, empty initials draw the person glyph, and a photo the avatar
// command answers replaces the monogram; without an address nothing pulls.
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ peopleAvatar: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  peopleAvatar: ipc.peopleAvatar,
}));

import { Avatar } from './Avatar';

beforeEach(() => {
  ipc.peopleAvatar.mockReset();
});

afterEach(cleanup);

describe('the avatar', () => {
  it('renders the row monogram initials on its colour', () => {
    render(<Avatar monogram={{ initials: 'AL', colour: '#4DB6AC' }} />);

    const badge = screen.getByRole('img', { name: 'Avatar' });
    expect(badge.style.backgroundColor).toBe('rgb(77, 182, 172)');
    expect(badge.textContent).toBe('AL');
    expect(ipc.peopleAvatar).not.toHaveBeenCalled();
  });

  it('draws the person glyph when the initials are empty', () => {
    render(<Avatar monogram={{ initials: '', colour: '#4DB6AC' }} />);

    const badge = screen.getByRole('img', { name: 'Avatar' });
    expect(badge.textContent).toBe('');
    expect(screen.getByRole('img', { name: 'Person' })).toBeTruthy();
  });

  it('replaces the monogram with the photo the command answers', async () => {
    ipc.peopleAvatar.mockResolvedValue({
      initials: 'AL',
      colour: '#4DB6AC',
      photo: 'C:\\blobs\\ada.png',
    });
    render(<Avatar monogram={{ initials: 'AL', colour: '#4DB6AC' }} address="ada@example.test" />);

    await waitFor(() => {
      expect(screen.getByRole('img', { name: 'Avatar' }).querySelector('img')).not.toBeNull();
    });
    const photo = screen.getByRole('img', { name: 'Avatar' }).querySelector('img');
    expect(photo?.getAttribute('src')).toContain(encodeURIComponent('C:\\blobs\\ada.png'));
    expect(screen.getByRole('img', { name: 'Avatar' }).textContent).toBe('');
  });

  it('keeps the monogram when the command answers no photo', async () => {
    ipc.peopleAvatar.mockResolvedValue({ initials: 'AL', colour: '#4DB6AC' });
    render(<Avatar monogram={{ initials: 'AL', colour: '#4DB6AC' }} address="ada@example.test" />);

    await waitFor(() => {
      expect(ipc.peopleAvatar).toHaveBeenCalledWith('ada@example.test');
    });
    expect(screen.getByRole('img', { name: 'Avatar' }).textContent).toBe('AL');
  });

  it('draws the resolved monogram when the caller carries none', async () => {
    ipc.peopleAvatar.mockResolvedValue({ initials: 'S', colour: '#F06292' });
    render(<Avatar address="stranger@example.test" />);

    await waitFor(() => {
      expect(screen.getByRole('img', { name: 'Avatar' }).textContent).toBe('S');
    });
    expect(screen.getByRole('img', { name: 'Avatar' }).style.backgroundColor).toBe(
      'rgb(240, 98, 146)',
    );
  });

  it('pulls again when the address moves', async () => {
    ipc.peopleAvatar.mockResolvedValue({ initials: 'AL', colour: '#4DB6AC' });
    const { rerender } = render(<Avatar address="ada@example.test" />);

    rerender(<Avatar address="bob@example.test" />);

    await waitFor(() => {
      expect(ipc.peopleAvatar).toHaveBeenNthCalledWith(2, 'bob@example.test');
    });
  });
});
