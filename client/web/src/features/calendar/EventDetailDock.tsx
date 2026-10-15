// SPDX-License-Identifier: GPL-3.0-only
// The event detail dock: what picking an event in any view opens, the
// mail-calendar detail conventions as a docked pane. Title, when (in the
// event's own wall clock under the clock setting, all-day shown as its
// inclusive days), the owning calendar with its colour, and the recurrence
// read-back the snapshot carries. Edit opens the event dialog; Delete runs
// the existing delete flow, asking the one-vs-all question on a series.
// Both actions stand only where the calendar is writable, and the dock is
// read-only otherwise.

import { useState } from 'react';

import { CloseIcon } from '@/components/icons';
import { dispatch, intents } from '@/services/ipc';
import type { OccurrenceRow } from '@/services/ipc';

import { useCalendarStore } from './calendarStore';
import { addDays, dayKeyOf, minutesInZone } from './calendarRange';
import { clockLabel, fullDayLabel } from './dayLabels';
import { CalendarDot, RepeatGlyph } from './EventMarkers';
import { DeleteChoice } from './EventDialogSteps';
import { useSettingsStore } from '@/features/settings/settingsStore';

/** The event's time as one line, in its own wall clock. */
function whenLine(row: OccurrenceRow, zone: string, twentyFour: boolean): string {
  const start = new Date(row.start);
  const startDay = dayKeyOf(start, zone);
  if (row.all_day) {
    // The stored end is exclusive; show the inclusive last day.
    const lastDay = addDays(dayKeyOf(new Date(row.end), zone), -1);
    return lastDay === startDay
      ? fullDayLabel(startDay)
      : `${fullDayLabel(startDay)} to ${fullDayLabel(lastDay)}`;
  }
  const end = new Date(row.end);
  const endDay = dayKeyOf(end, zone);
  const startClock = clockLabel(minutesInZone(start, zone), twentyFour);
  const endClock = clockLabel(minutesInZone(end, zone), twentyFour);
  return endDay === startDay
    ? `${fullDayLabel(startDay)}, ${startClock} to ${endClock}`
    : `${fullDayLabel(startDay)} ${startClock} to ${fullDayLabel(endDay)} ${endClock}`;
}

/** One labelled row of the detail, the label over the value. */
function Labeled({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] font-semibold uppercase tracking-wide text-primary">
        {label}
      </span>
      <span className="text-[13px] text-foreground">{children}</span>
    </div>
  );
}

/** The dock's body for the picked occurrence. */
export function EventDetailDock({
  onEdit,
}: {
  /** Opens the event dialog over the picked occurrence. */
  onEdit: (row: OccurrenceRow) => void;
}) {
  const row = useCalendarStore((s) => s.selectedEvent);
  const calendars = useCalendarStore((s) => s.calendars);
  const nowData = useCalendarStore((s) => s.nowData);
  const clearSelection = useCalendarStore((s) => s.clearSelection);
  const twentyFour = useSettingsStore((s) => s.settings.twenty_four_clock);
  const [deleteChoice, setDeleteChoice] = useState(false);

  if (row === null) {
    return null;
  }

  const zone = nowData?.zone ?? 'Etc/UTC';
  const calendar = calendars.find(
    (candidate) => candidate.account === row.account && candidate.id === row.calendar,
  );
  const writable = calendar?.can_write ?? false;
  const scopeable = row.recurring && row.occurrence_start !== undefined;

  const remove = (occurrence: boolean): void => {
    void dispatch(
      intents.deleteCalendarEvent(
        { account: row.account, key: row.event },
        occurrence ? row.occurrence_start : undefined,
      ),
    );
    clearSelection();
  };

  return (
    <aside aria-label="Event detail" className="flex h-full flex-col overflow-hidden">
      <div className="flex shrink-0 items-center justify-end border-b border-border-subtle px-2 py-1">
        <button
          type="button"
          aria-label="Close event detail"
          onClick={clearSelection}
          className="flex h-6 w-6 items-center justify-center rounded-[var(--radius-xs)] text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <CloseIcon size={12} />
        </button>
      </div>
      {deleteChoice ? (
        <div className="p-4">
          <DeleteChoice onBack={() => setDeleteChoice(false)} onDelete={remove} />
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4">
          <h2 className="flex items-center gap-1.5 text-sm font-semibold text-foreground">
            {row.recurring && (
              <span title={`${row.title || '(no title)'} repeats`} className="flex items-center">
                <RepeatGlyph />
              </span>
            )}
            {row.title === '' ? '(no title)' : row.title}
          </h2>
          <div className="flex flex-col gap-0.5">
            <span className="text-[13px] text-foreground">{whenLine(row, zone, twentyFour)}</span>
            {row.accuracy === 'zoned' && (
              <span className="text-[11px] text-muted-foreground">{zone}</span>
            )}
          </div>
          <Labeled label="Calendar">
            <span className="flex items-center gap-2">
              <CalendarDot color={calendar?.color} />
              {calendar?.name ?? row.calendar}
            </span>
          </Labeled>
          {row.recurring && <Labeled label="Repeat">Repeats. The series holds its own rule.</Labeled>}
          {writable && (
            <div className="mt-auto flex items-center justify-between gap-2 border-t border-border-subtle pt-3">
              <button
                type="button"
                onClick={() => (scopeable ? setDeleteChoice(true) : remove(false))}
                className="rounded-[var(--radius)] border border-[var(--destructive)] px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                Delete
              </button>
              <button
                type="button"
                onClick={() => onEdit(row)}
                className="rounded-[var(--radius)] bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground shadow-[var(--shadow-sm)] transition-colors hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                Edit
              </button>
            </div>
          )}
        </div>
      )}
    </aside>
  );
}
