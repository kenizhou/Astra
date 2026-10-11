// SPDX-License-Identifier: GPL-3.0-only
// The work-week view: the shared time grid over the anchor week's five
// weekdays, Monday to Friday.

import type { OccurrenceRow } from '@/services/ipc';

import { useCalendarStore } from './calendarStore';
import { dayMonthLabel } from './dayLabels';
import { parseDayKey, workWeekOf } from './calendarRange';
import { TimeGrid } from './TimeGrid';

/** The work week's grid, anchored on the store's navigation anchor. */
export function WorkWeekGrid({ onOpenEvent }: { onOpenEvent: (row: OccurrenceRow) => void }) {
  const anchor = useCalendarStore((s) => s.anchor);

  if (anchor === null) {
    return <p className="p-4 text-sm text-muted-foreground">The calendar has no clock yet.</p>;
  }
  const days = workWeekOf(anchor);
  const label = `Work week grid, ${dayMonthLabel(days[0]!)} to ${dayMonthLabel(days[4]!)} ${parseDayKey(days[0]!).year}`;
  return <TimeGrid days={days} ariaLabel={label} onOpenEvent={onOpenEvent} />;
}
