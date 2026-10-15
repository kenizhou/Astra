// SPDX-License-Identifier: GPL-3.0-only
// The calendar rail: the mini-month navigator over the calendar visibility
// list, one row per calendar with its colour chip and a checkbox naming it
// shown or hidden through the same intent the toolbar's manager dispatches.

import { useCalendarStore } from './calendarStore';
import { setCalendarVisible } from './calendarVisibility';
import { CalendarDot } from './EventMarkers';
import { MiniMonth } from './MiniMonth';

/** The rail's calendar list with its visibility checkboxes. */
function CalendarList() {
  const calendars = useCalendarStore((s) => s.calendars);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto p-2">
      <h3 className="px-2 pb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-text">
        Calendars
      </h3>
      {calendars.map((calendar) => (
        <label
          key={`${calendar.account}:${calendar.id}`}
          className="flex items-center gap-2 rounded-[var(--radius-xs)] px-2 py-1 text-xs transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <input
            type="checkbox"
            aria-label={`Show ${calendar.name}`}
            checked={calendar.visible}
            onChange={(event) =>
              setCalendarVisible(calendars, calendar.account, calendar.id, event.target.checked)
            }
            className="h-3.5 w-3.5 shrink-0 accent-[var(--primary)]"
          />
          <CalendarDot color={calendar.color} />
          <span className="min-w-0 flex-1 truncate text-foreground">{calendar.name}</span>
        </label>
      ))}
      {calendars.length === 0 && (
        <p className="px-2 py-1 text-xs text-muted-foreground">No calendar yet.</p>
      )}
    </div>
  );
}

/** The calendar's left rail: the mini-month over the calendar list. */
export function CalendarRail() {
  return (
    <div aria-label="Calendar rail" className="flex h-full flex-col overflow-hidden">
      <MiniMonth />
      <CalendarList />
    </div>
  );
}
