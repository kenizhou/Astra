// SPDX-License-Identifier: GPL-3.0-only
// The snooze wire: the command asks by name with the message halves and the
// wake instant in the camelCase the host's command macro reads, and the
// picker's wake choices compute the three instants over a fixed now.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { snoozeChoices, snoozeMessage } from './snooze';

beforeEach(() => {
  invoke.mockReset();
});

describe('the snooze command', () => {
  it('asks by name with the message halves and the wake instant', async () => {
    invoke.mockResolvedValue(undefined);
    await snoozeMessage({ account: 'work', key: 'm1' }, 1_763_000_000);
    expect(invoke).toHaveBeenCalledWith('snooze_message', {
      account: 'work',
      key: 'm1',
      wakeAt: 1_763_000_000,
    });
  });
});

describe('the wake choices', () => {
  it('computes later today, tomorrow, and next week as rising instants', () => {
    const now = new Date('2026-11-12T12:00:00Z');
    const choices = snoozeChoices(now);
    expect(choices.map((choice) => choice.label)).toEqual([
      'Later today',
      'Tomorrow',
      'Next week',
    ]);
    const [later, tomorrow, nextWeek] = choices;
    expect(later!.wakeAt).toBeLessThan(tomorrow!.wakeAt);
    expect(tomorrow!.wakeAt).toBeLessThan(nextWeek!.wakeAt);
    // Tomorrow is the next civil day at eight in the morning, local.
    const expected = new Date(now);
    expected.setDate(expected.getDate() + 1);
    expected.setHours(8, 0, 0, 0);
    expect(tomorrow!.wakeAt).toBe(Math.floor(expected.getTime() / 1000));
  });

  it('keeps every choice in the future even in the evening', () => {
    // Past the 18:00 "later today" hour: a naive this-evening instant would
    // lie in the past and the message would return on the next tick.
    const now = new Date('2026-11-12T12:00:00Z');
    now.setHours(20, 0, 0, 0);
    const nowUnix = Math.floor(now.getTime() / 1000);
    for (const choice of snoozeChoices(now)) {
      expect(choice.wakeAt).toBeGreaterThan(nowUnix);
    }
  });
});