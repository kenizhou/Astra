// SPDX-License-Identifier: GPL-3.0-only
// The work-week view: the shared time grid over the anchor week's first
// five days, the week opening on the day the settings block names. A
// Sunday-first week lays out Sunday to Thursday, and a Wednesday or
// Friday first day crosses the weekend: the locale's own week edge
// decides the window, never a hardcoded Monday start.

import type { OccurrenceRow } from '@/services/ipc';

import { useSettingsStore } from '@/features/settings/settingsStore';
import { useCalendarStore } from './calendarStore';
import { dayMonthLabel, firstDayIndex } from './dayLabels';
import { parseDayKey, workWeekOf } from './calendarRange';
import { TimeGrid } from './TimeGrid';
import type { CreateSlot } from './useEventDrag';

interface WorkWeekGridProps {
  /** Picks an event for the detail dock. */
  onSelectEvent: (row: OccurrenceRow) => void;
  /** Opens the editor prefilled with a drag-created slot. */
  onCreateSlot?: (slot: CreateSlot) => void;
}

/** The work week's grid, anchored on the store's navigation anchor. */
export function WorkWeekGrid({ onSelectEvent, onCreateSlot }: WorkWeekGridProps) {
  const anchor = useCalendarStore((s) => s.anchor);
  const firstDay = firstDayIndex(useSettingsStore((s) => s.settings.first_day_of_week));

  if (anchor === null) {
    return <p className="p-4 text-sm text-muted-foreground">The calendar has no clock yet.</p>;
  }
  const days = workWeekOf(anchor, firstDay);
  const label = `Work week grid, ${dayMonthLabel(days[0]!)} to ${dayMonthLabel(days[4]!)} ${parseDayKey(days[0]!).year}`;
  return (
    <TimeGrid
      days={days}
      ariaLabel={label}
      onSelectEvent={onSelectEvent}
      onCreateSlot={onCreateSlot}
    />
  );
}
