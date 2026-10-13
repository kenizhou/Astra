// SPDX-License-Identifier: GPL-3.0-only
// The day view: the shared time grid over the anchor's single civil day.

import type { OccurrenceRow } from '@/services/ipc';

import { useCalendarStore } from './calendarStore';
import { fullDayLabel } from './dayLabels';
import { TimeGrid } from './TimeGrid';
import type { CreateSlot } from './useEventDrag';

interface DayGridProps {
  /** Picks an event for the detail dock. */
  onSelectEvent: (row: OccurrenceRow) => void;
  /** Opens the editor prefilled with a drag-created slot. */
  onCreateSlot?: (slot: CreateSlot) => void;
}

/** One day's grid, anchored on the store's navigation anchor. */
export function DayGrid({ onSelectEvent, onCreateSlot }: DayGridProps) {
  const anchor = useCalendarStore((s) => s.anchor);

  if (anchor === null) {
    return <p className="p-4 text-sm text-muted-foreground">The calendar has no clock yet.</p>;
  }
  return (
    <TimeGrid
      days={[anchor]}
      ariaLabel={`Day grid, ${fullDayLabel(anchor)}`}
      onSelectEvent={onSelectEvent}
      onCreateSlot={onCreateSlot}
    />
  );
}
