// SPDX-License-Identifier: GPL-3.0-only
// The day view: the shared time grid over the anchor's single civil day.

import type { OccurrenceRow } from '@/services/ipc';

import { useCalendarStore } from './calendarStore';
import { fullDayLabel } from './dayLabels';
import { TimeGrid } from './TimeGrid';

/** One day's grid, anchored on the store's navigation anchor. */
export function DayGrid({ onOpenEvent }: { onOpenEvent: (row: OccurrenceRow) => void }) {
  const anchor = useCalendarStore((s) => s.anchor);

  if (anchor === null) {
    return <p className="p-4 text-sm text-muted-foreground">The calendar has no clock yet.</p>;
  }
  return <TimeGrid days={[anchor]} ariaLabel={`Day grid, ${fullDayLabel(anchor)}`} onOpenEvent={onOpenEvent} />;
}
