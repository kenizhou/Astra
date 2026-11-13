// SPDX-License-Identifier: GPL-3.0-only
// The snooze dialog: the three computed wake choices, a choice firing the
// host's snooze command with the message and the chosen instant and closing,
// and the Cancel and Escape dismissals that dispatch nothing. The wake
// instants are read from the same pure helper the dialog uses, so the
// assertion holds in any display zone.
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const snooze = vi.hoisted(() => ({
  snoozeMessage: vi.fn(),
}));

vi.mock('@/services/snooze', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...snooze,
}));

import { SnoozeDialog } from './SnoozeDialog';
import { useSnoozeStore } from './snoozeStore';
import { snoozeChoices } from '@/services/snooze';
import type { MessageRef } from '@/services/ipc';

const target: MessageRef = { account: 'work', key: 'm1' };

/** Opens the dialog over the message, as the row menu entry does. */
function open(): void {
  useSnoozeStore.getState().open(target);
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-11-12T12:00:00Z'));
  snooze.snoozeMessage.mockReset();
  snooze.snoozeMessage.mockResolvedValue(undefined);
  useSnoozeStore.setState({ target: null });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('the snooze dialog', () => {
  it('renders nothing while no message stands', () => {
    render(<SnoozeDialog />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('names the three wake choices', () => {
    open();
    render(<SnoozeDialog />);

    const dialog = screen.getByRole('dialog', { name: 'Snooze message' });
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    const group = within(dialog).getByRole('group', { name: 'Wake time' });
    expect(within(group).getByRole('button', { name: 'Later today' })).toBeTruthy();
    expect(within(group).getByRole('button', { name: 'Tomorrow' })).toBeTruthy();
    expect(within(group).getByRole('button', { name: 'Next week' })).toBeTruthy();
  });

  it('a choice snoozes the message at the computed instant and closes', () => {
    open();
    render(<SnoozeDialog />);
    const choices = snoozeChoices(new Date());

    fireEvent.click(screen.getByRole('button', { name: 'Tomorrow' }));

    expect(snooze.snoozeMessage).toHaveBeenCalledTimes(1);
    expect(snooze.snoozeMessage).toHaveBeenCalledWith(target, choices[1]!.wakeAt);
    expect(useSnoozeStore.getState().target).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('Cancel closes without dispatching', () => {
    open();
    render(<SnoozeDialog />);

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(snooze.snoozeMessage).not.toHaveBeenCalled();
    expect(useSnoozeStore.getState().target).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('Escape closes without dispatching', () => {
    open();
    render(<SnoozeDialog />);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });

    expect(snooze.snoozeMessage).not.toHaveBeenCalled();
    expect(useSnoozeStore.getState().target).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});