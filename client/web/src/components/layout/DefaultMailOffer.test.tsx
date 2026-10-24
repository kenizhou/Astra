// SPDX-License-Identifier: GPL-3.0-only
// The default-mail offer: asked once, only when an account stands and the
// host's latch is unset; both answers mark the ask spent, the acceptance
// re-asserts the registration through the host verb and surfaces its
// outcome, and a refused latch read never nags.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: the banner asks only inside Tauri, so the jsdom
  // test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const platform = vi.hoisted(() => ({
  defaultMailAsked: vi.fn(),
  markDefaultMailAsked: vi.fn(),
  registerDefaultMail: vi.fn(),
}));

vi.mock('@/services/platform', () => ({
  defaultMailAsked: platform.defaultMailAsked,
  markDefaultMailAsked: platform.markDefaultMailAsked,
  registerDefaultMail: platform.registerDefaultMail,
}));

import { DefaultMailOffer } from './DefaultMailOffer';
import { useAccountStore } from '@/stores/accountStore';

function standAccount(): void {
  act(() => {
    useAccountStore.getState().applySnapshot({
      account_folders: [
        {
          account_id: 'work',
          display_name: 'Ada',
          address: 'ada@example.test',
          expanded: false,
          folders: [],
        },
      ],
    });
  });
}

beforeEach(() => {
  platform.defaultMailAsked.mockReset();
  platform.defaultMailAsked.mockResolvedValue(false);
  platform.markDefaultMailAsked.mockReset();
  platform.markDefaultMailAsked.mockResolvedValue(undefined);
  platform.registerDefaultMail.mockReset();
  platform.registerDefaultMail.mockResolvedValue('registered');
  act(() => {
    useAccountStore.getState().applySnapshot({ account_folders: [] });
  });
});

afterEach(() => {
  cleanup();
});

describe('the default-mail offer', () => {
  it('never asks before an account stands', async () => {
    render(<DefaultMailOffer />);

    await waitFor(() =>
      expect(platform.defaultMailAsked).not.toHaveBeenCalled(),
    );
    expect(screen.queryByRole('dialog', { name: 'Default mail app' })).toBeNull();
  });

  it('asks once when an account stands and the latch is unset', async () => {
    render(<DefaultMailOffer />);
    standAccount();

    expect(await screen.findByRole('dialog', { name: 'Default mail app' })).toBeTruthy();
    expect(platform.defaultMailAsked).toHaveBeenCalledTimes(1);
  });

  it('never asks when the latch already stands', async () => {
    platform.defaultMailAsked.mockResolvedValue(true);
    render(<DefaultMailOffer />);
    standAccount();

    await waitFor(() => expect(platform.defaultMailAsked).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole('dialog', { name: 'Default mail app' })).toBeNull();
  });

  it('never asks when the latch read refuses', async () => {
    platform.defaultMailAsked.mockRejectedValue(new Error('the latch read refused'));
    render(<DefaultMailOffer />);
    standAccount();

    await waitFor(() => expect(platform.defaultMailAsked).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole('dialog', { name: 'Default mail app' })).toBeNull();
  });

  it('marks the ask spent and registers on acceptance, surfacing the outcome', async () => {
    render(<DefaultMailOffer />);
    standAccount();

    fireEvent.click(
      await screen.findByRole('button', { name: 'Use Astra Mail for mail links' }),
    );

    await waitFor(() => expect(platform.markDefaultMailAsked).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(platform.registerDefaultMail).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/is the default mail app/)).toBeTruthy();
  });

  it('marks the ask spent on a pass and never registers', async () => {
    render(<DefaultMailOffer />);
    standAccount();

    fireEvent.click(await screen.findByRole('button', { name: 'Not now' }));

    await waitFor(() => expect(platform.markDefaultMailAsked).toHaveBeenCalledTimes(1));
    expect(platform.registerDefaultMail).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog', { name: 'Default mail app' })).toBeNull();
  });
});
