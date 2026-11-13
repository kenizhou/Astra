// SPDX-License-Identifier: GPL-3.0-only
// The reminder dialog: the three computed wake choices (the snooze preset
// math reused), a choice firing the host's remind command with the message
// and the chosen instant and closing, and the Cancel and Escape dismissals
// that dispatch nothing. The wake instants are read from the same pure
// helper the dialog uses, so the assertion holds in any display zone.
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const reminders = vi.hoisted(() => ({
  remindMessage: vi.fn(),
}));

vi.mock('@/services/reminders', () => reminders);

import { ReminderDialog } from './ReminderDialog';
import { useReminderStore } from './reminderStore';
import { snoozeChoices } from '@/services/snooze';
import type { MessageRef } from '@/services/ipc';

const target: MessageRef = { account: 'work', key: 'm1' };

/** Opens the dialog over the message, as the row menu entry does. */
function open(): void {
  useReminderStore.getState().open(target);
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-11-12T12:00:00Z'));
  reminders.remindMessage.mockReset();
  reminders.remindMessage.mockResolvedValue(undefined);
  useReminderStore.setState({ target: null });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('the reminder dialog', () => {
  it('renders nothing while no message stands', () => {
    render(<ReminderDialog />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('names the three wake choices', () => {
    open();
    render(<ReminderDialog />);

    const dialog = screen.getByRole('dialog', { name: 'Remind me' });
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    const group = within(dialog).getByRole('group', { name: 'Wake time' });
    expect(within(group).getByRole('button', { name: 'Later today' })).toBeTruthy();
    expect(within(group).getByRole('button', { name: 'Tomorrow' })).toBeTruthy();
    expect(within(group).getByRole('button', { name: 'Next week' })).toBeTruthy();
  });

  it('a choice reminds about the message at the computed instant and closes', () => {
    open();
    render(<ReminderDialog />);
    const choices = snoozeChoices(new Date());

    fireEvent.click(screen.getByRole('button', { name: 'Tomorrow' }));

    expect(reminders.remindMessage).toHaveBeenCalledTimes(1);
    expect(reminders.remindMessage).toHaveBeenCalledWith(target, choices[1]!.wakeAt);
    expect(useReminderStore.getState().target).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('Cancel closes without dispatching', () => {
    open();
    render(<ReminderDialog />);

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(reminders.remindMessage).not.toHaveBeenCalled();
    expect(useReminderStore.getState().target).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('Escape closes without dispatching', () => {
    open();
    render(<ReminderDialog />);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });

    expect(reminders.remindMessage).not.toHaveBeenCalled();
    expect(useReminderStore.getState().target).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
