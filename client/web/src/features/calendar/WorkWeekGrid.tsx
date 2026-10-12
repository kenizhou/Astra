// SPDX-License-Identifier: GPL-3.0-only
// The work-week view: the shared time grid over the anchor week's first
// five days, the week opening on the day the settings block names.

import type { OccurrenceRow } from '@/services/ipc';

import { useSettingsStore } from '@/features/settings/settingsStore';
import { useCalendarStore } from './calendarStore';
import { dayMonthLabel, firstDayIndex } from './dayLabels';
import { parseDayKey, workWeekOf } from './calendarRange';
import { TimeGrid } from './TimeGrid';

/** The work week's grid, anchored on the store's navigation anchor. */
export function WorkWeekGrid({ onOpenEvent }: { onOpenEvent: (row: OccurrenceRow) => void }) {
  const anchor = useCalendarStore((s) => s.anchor);
  const firstDay = firstDayIndex(useSettingsStore((s) => s.settings.first_day_of_week));

  if (anchor === null) {
    return <p className="p-4 text-sm text-muted-foreground">The calendar has no clock yet.</p>;
  }
  const days = workWeekOf(anchor, firstDay);
  const label = `Work week grid, ${dayMonthLabel(days[0]!)} to ${dayMonthLabel(days[4]!)} ${parseDayKey(days[0]!).year}`;
  return <TimeGrid days={days} ariaLabel={label} onOpenEvent={onOpenEvent} />;
}
