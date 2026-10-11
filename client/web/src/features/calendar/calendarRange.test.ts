// SPDX-License-Identifier: GPL-3.0-only
// The calendar layout math: civil-day keys localised into the snapshot's own
// zone, the month grid and week rows over them, the wall-clock to instant
// conversion a window anchor needs, and the drag snapping with the same
// 366-day bound the core's drag arithmetic enforces, beside the optimistic
// shift a committed drag renders from.
import { describe, expect, it } from 'vitest';

import {
  MAX_DRAG_DAYS,
  SLOT_MINUTES,
  addDays,
  addMonths,
  dayKeyOf,
  dragFromGrid,
  instantAfterDrag,
  instantOfZoned,
  minutesInZone,
  monthGrid,
  parseDayKey,
  snapToSlot,
  weekOf,
  weekdayOfKey,
  workWeekOf,
  zoneToday,
} from './calendarRange';

describe('zoned day keys', () => {
  it('localises the same instant into the snapshot zone, not the host zone', () => {
    // Half past ten in the evening in UTC is already the next day in
    // Amsterdam and still the same day in New York.
    const at = new Date('2026-10-19T22:30:00Z');
    expect(dayKeyOf(at, 'Europe/Amsterdam')).toBe('2026-10-20');
    expect(dayKeyOf(at, 'America/New_York')).toBe('2026-10-19');
  });

  it('reads the zone-local clock for placement', () => {
    const at = new Date('2026-10-19T07:00:00Z');
    expect(minutesInZone(at, 'Europe/Amsterdam')).toBe(9 * 60);
    expect(minutesInZone(at, 'America/New_York')).toBe(3 * 60);
  });

  it('names the zone-local today of a snapshot instant', () => {
    expect(zoneToday('2026-10-19T22:30:00Z', 'Europe/Amsterdam')).toBe('2026-10-20');
    expect(zoneToday('2026-10-19T22:30:00Z', 'Etc/UTC')).toBe('2026-10-19');
  });

  it('parses a day key back into its civil date', () => {
    expect(parseDayKey('2026-10-19')).toEqual({ year: 2026, month: 10, day: 19 });
  });
});

describe('civil day arithmetic', () => {
  it('adds days across month and year boundaries', () => {
    expect(addDays('2026-10-31', 1)).toBe('2026-11-01');
    expect(addDays('2026-12-31', 1)).toBe('2027-01-01');
    expect(addDays('2026-03-01', -1)).toBe('2026-02-28');
  });

  it('shifts months clamping the day when the target is shorter', () => {
    expect(addMonths('2026-01-31', 1)).toBe('2026-02-28');
    expect(addMonths('2026-10-15', -9)).toBe('2026-01-15');
    expect(addMonths('2026-12-15', 2)).toBe('2027-02-15');
  });

  it('names the weekday of a day key, Monday as the week start', () => {
    expect(weekdayOfKey('2026-10-19')).toBe(1);
    expect(weekdayOfKey('2026-10-18')).toBe(0);
    expect(weekdayOfKey('2026-10-24')).toBe(6);
  });
});

describe('week rows', () => {
  it('spans the Monday-start week holding the anchor', () => {
    // 2026-10-21 is a Wednesday; its week runs Monday the 19th to Sunday
    // the 25th, the Sunday the clocks change in Europe, and every key stays
    // a civil day: daylight saving never moves a date.
    expect(weekOf('2026-10-21')).toEqual([
      '2026-10-19',
      '2026-10-20',
      '2026-10-21',
      '2026-10-22',
      '2026-10-23',
      '2026-10-24',
      '2026-10-25',
    ]);
  });

  it('keeps the week civil across the spring-forward boundary', () => {
    // The last Sunday of March 2026 is the 29th, the day the clocks go
    // forward; the week holding it is unchanged civil dates.
    expect(weekOf('2026-03-29')).toEqual([
      '2026-03-23',
      '2026-03-24',
      '2026-03-25',
      '2026-03-26',
      '2026-03-27',
      '2026-03-28',
      '2026-03-29',
    ]);
  });

  it('cuts the work week to its five weekdays', () => {
    expect(workWeekOf('2026-10-21')).toEqual([
      '2026-10-19',
      '2026-10-20',
      '2026-10-21',
      '2026-10-22',
      '2026-10-23',
    ]);
  });
});

describe('the month grid', () => {
  it('lays out six weeks from the Monday before the first of the month', () => {
    // October 2026 opens on a Thursday, so the grid starts Monday the 28th
    // of September and its 42 cells end on Sunday the 8th of November.
    const grid = monthGrid('2026-10-15');
    expect(grid).toHaveLength(42);
    expect(grid[0]).toBe('2026-09-28');
    expect(grid[41]).toBe('2026-11-08');
    // Every row is a Monday-start week of civil days.
    expect(weekdayOfKey(grid[0]!)).toBe(1);
    expect(weekdayOfKey(grid[7]!)).toBe(1);
    expect(weekdayOfKey(grid[41]!)).toBe(0);
  });

  it('holds the whole month the anchor names whatever day it sits on', () => {
    const grid = monthGrid('2026-10-01');
    expect(grid.some((key) => key === '2026-10-31')).toBe(true);
    const december = monthGrid('2026-12-25');
    expect(december[0]).toBe('2026-11-30');
    expect(december[41]).toBe('2027-01-10');
  });
});

describe('wall clocks in the zone', () => {
  it('resolves a zone wall clock to its UTC instant through the offsets', () => {
    // Amsterdam sits on CEST (+2) on the 24th and on CET (+1) once the
    // clocks go back in the small hours of the 25th: the same wall clock is
    // a different instant either side.
    expect(instantOfZoned('2026-10-24', 9 * 60, 'Europe/Amsterdam')).toBe('2026-10-24T07:00:00Z');
    expect(instantOfZoned('2026-10-25', 9 * 60, 'Europe/Amsterdam')).toBe('2026-10-25T08:00:00Z');
  });

  it('resolves the same wall clock in the zone the window opens at', () => {
    expect(instantOfZoned('2026-10-19', 0, 'Europe/Amsterdam')).toBe('2026-10-18T22:00:00Z');
    expect(instantOfZoned('2026-10-19', 0, 'Etc/UTC')).toBe('2026-10-19T00:00:00Z');
  });
});

describe('drag snapping', () => {
  it('snaps minutes onto the slot grid', () => {
    expect(SLOT_MINUTES).toBe(15);
    expect(snapToSlot(7)).toBe(0);
    expect(snapToSlot(14)).toBe(0);
    expect(snapToSlot(15)).toBe(15);
    expect(snapToSlot(52)).toBe(45);
    expect(snapToSlot(-7)).toBe(-15);
  });

  it('turns grid positions into the drag delta the core takes', () => {
    // Monday 09:07 dragged to Tuesday 08:37: a day and minus thirty minutes
    // once both ends are snapped to the slot grid.
    expect(
      dragFromGrid(
        { day: 0, minutes: 9 * 60 + 7 },
        { day: 1, minutes: 8 * 60 + 37 },
      ),
    ).toEqual({ days: 1, minutes: -30 });
    expect(
      dragFromGrid(
        { day: 3, minutes: 10 * 60 },
        { day: 1, minutes: 10 * 60 + 45 },
      ),
    ).toEqual({ days: -2, minutes: 45 });
  });

  it('refuses a drag beyond the 366-day bound the core enforces', () => {
    expect(MAX_DRAG_DAYS).toBe(366);
    expect(dragFromGrid({ day: 0, minutes: 0 }, { day: 366, minutes: 30 })).toEqual({
      days: 366,
      minutes: 30,
    });
    // A span past the bound answers null: the gesture never crosses the IPC.
    expect(dragFromGrid({ day: 0, minutes: 0 }, { day: 400, minutes: 0 })).toBeNull();
    expect(dragFromGrid({ day: 400, minutes: 0 }, { day: 0, minutes: 0 })).toBeNull();
    // A negative-minute remainder carries a day the trunc would drop; the
    // floor matches the core's div_euclid, so the refusal agrees on both sides.
    expect(dragFromGrid({ day: 0, minutes: 15 }, { day: -366, minutes: -15 })).toBeNull();
  });
});

describe('the optimistic drag shift', () => {
  it('moves an instant by a wall-clock delta in the zone', () => {
    // 09:00 in Amsterdam, half an hour later.
    expect(instantAfterDrag('2026-10-19T07:00:00Z', 0, 30, 'Europe/Amsterdam')).toBe(
      '2026-10-19T07:30:00Z',
    );
    // Backwards, before midnight: 00:15 on the 19th less an hour is 23:15 on
    // the 18th.
    expect(instantAfterDrag('2026-10-18T22:15:00Z', 0, -60, 'Europe/Amsterdam')).toBe(
      '2026-10-18T21:15:00Z',
    );
  });

  it('carries the clock across midnight in both directions', () => {
    // 23:30 plus an hour is 00:30 the next civil day.
    expect(instantAfterDrag('2026-10-19T21:30:00Z', 0, 60, 'Europe/Amsterdam')).toBe(
      '2026-10-19T22:30:00Z',
    );
    expect(instantAfterDrag('2026-10-19T22:30:00Z', 0, 60, 'Europe/Amsterdam')).toBe(
      '2026-10-19T23:30:00Z',
    );
  });

  it('keeps the wall clock over a whole-day move, DST included', () => {
    // 09:00 on Saturday carried to Sunday keeps 09:00 across the fall-back.
    expect(instantAfterDrag('2026-10-24T07:00:00Z', 1, 0, 'Europe/Amsterdam')).toBe(
      '2026-10-25T08:00:00Z',
    );
  });
});
