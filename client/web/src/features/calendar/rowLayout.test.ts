// SPDX-License-Identifier: GPL-3.0-only
// The day blocks: occurrence rows placed into one zone-local civil day, the
// all-day lane split from the timed blocks, and overlapping timed blocks
// packed side by side so nothing renders on top of anything else.
import { describe, expect, it } from 'vitest';

import { dayBlocks } from './rowLayout';
import type { OccurrenceRow } from '@/services/ipc';

/** A timed occurrence row on 19 October 2026, Amsterdam. */
function row(over: Partial<OccurrenceRow> = {}): OccurrenceRow {
  return {
    account: 'work',
    event: '/dav/cal/work/default/evt-1.ics',
    calendar: 'cal-1',
    title: 'Standup',
    start: '2026-10-19T07:00:00Z',
    end: '2026-10-19T07:30:00Z',
    all_day: false,
    recurring: false,
    overridden: false,
    accuracy: 'zoned',
    participation: 'accepted',
    can_move: true,
    series_edit_warns: false,
    ...over,
  };
}

const ZONE = 'Europe/Amsterdam';

describe('the day blocks', () => {
  it('places a timed row by its zone-local clock', () => {
    // 07:00Z is 09:00 in Amsterdam on 19 October 2026.
    const { allDay, timed } = dayBlocks([row()], '2026-10-19', ZONE);

    expect(allDay).toEqual([]);
    expect(timed).toHaveLength(1);
    expect(timed[0]!.startMinute).toBe(9 * 60);
    expect(timed[0]!.endMinute).toBe(9 * 60 + 30);
    expect(timed[0]!.continuesBefore).toBe(false);
    expect(timed[0]!.continuesAfter).toBe(false);
  });

  it('keeps a row off days it does not touch', () => {
    expect(dayBlocks([row()], '2026-10-20', ZONE).timed).toEqual([]);
    // An event ending exactly at midnight belongs to the day before.
    const endsMidnight = row({
      start: '2026-10-19T21:00:00Z',
      end: '2026-10-19T22:00:00Z',
    });
    expect(dayBlocks([endsMidnight], '2026-10-19', ZONE).timed).toHaveLength(1);
    expect(dayBlocks([endsMidnight], '2026-10-20', ZONE).timed).toEqual([]);
  });

  it('clamps a row that runs past the day it is drawn in', () => {
    // Starts 20:00 zone time on the 19th, ends 04:00 zone time on the 20th.
    const spans = row({
      start: '2026-10-19T18:00:00Z',
      end: '2026-10-20T02:00:00Z',
    });
    const first = dayBlocks([spans], '2026-10-19', ZONE).timed[0]!;
    expect(first.startMinute).toBe(20 * 60);
    expect(first.endMinute).toBe(24 * 60);
    expect(first.continuesAfter).toBe(true);
    const second = dayBlocks([spans], '2026-10-20', ZONE).timed[0]!;
    expect(second.startMinute).toBe(0);
    expect(second.endMinute).toBe(4 * 60);
    expect(second.continuesBefore).toBe(true);
  });

  it('rides an all-day row in the lane, once per day it covers', () => {
    // All-day rows resolve to local midnights: the 19th to the 21st in UTC
    // is the 19th and the 20th in Amsterdam.
    const allday = row({
      all_day: true,
      start: '2026-10-18T22:00:00Z',
      end: '2026-10-20T22:00:00Z',
      accuracy: 'all_day',
    });
    expect(dayBlocks([allday], '2026-10-19', ZONE).allDay).toEqual([allday]);
    expect(dayBlocks([allday], '2026-10-20', ZONE).allDay).toEqual([allday]);
    expect(dayBlocks([allday], '2026-10-21', ZONE).allDay).toEqual([]);
  });

  it('packs overlapping rows side by side and widens the lane for both', () => {
    const early = row({ event: 'a', title: 'Early', start: '2026-10-19T07:00:00Z', end: '2026-10-19T08:00:00Z' });
    const overlapping = row({ event: 'b', title: 'Clash', start: '2026-10-19T07:15:00Z', end: '2026-10-19T07:45:00Z' });
    const later = row({ event: 'c', title: 'Later', start: '2026-10-19T09:00:00Z', end: '2026-10-19T10:00:00Z' });

    const { timed } = dayBlocks([early, overlapping, later], '2026-10-19', ZONE);

    // The two overlapping rows share the 09:00 hour in columns; the later row
    // stands alone in one full-width column of its own.
    const earlyBlock = timed.find((block) => block.row.title === 'Early')!;
    const clashBlock = timed.find((block) => block.row.title === 'Clash')!;
    const laterBlock = timed.find((block) => block.row.title === 'Later')!;
    expect(earlyBlock.column).toBe(0);
    expect(clashBlock.column).toBe(1);
    expect(earlyBlock.columns).toBe(2);
    expect(clashBlock.columns).toBe(2);
    expect(laterBlock.column).toBe(0);
    expect(laterBlock.columns).toBe(1);
  });

  it('sorts the timed rows by their start', () => {
    const later = row({ event: 'c', start: '2026-10-19T09:00:00Z', end: '2026-10-19T10:00:00Z' });
    const early = row({ event: 'a', start: '2026-10-19T07:00:00Z', end: '2026-10-19T08:00:00Z' });
    const { timed } = dayBlocks([later, early], '2026-10-19', ZONE);
    expect(timed.map((block) => block.row.event)).toEqual(['a', 'c']);
  });
});
