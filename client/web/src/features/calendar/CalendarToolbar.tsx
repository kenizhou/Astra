// SPDX-License-Identifier: GPL-3.0-only
// The calendar toolbar: the navigation verbs (Today, the period arrows, the
// view switch), the New event and Briefs actions, and the calendar manager
// whose colour dots and visibility toggles decide which calendars draw. The
// chrome is Astra's own; the layout stack is kylins' CalendarToolbar adapted.

import { useEffect, useRef, useState } from 'react';

import { dispatch, intents } from '@/services/ipc';

import { addDays, addMonths, zoneToday } from './calendarRange';
import { useCalendarStore, type CalendarView } from './calendarStore';
import { monthYearLabel, firstDayIndex } from './dayLabels';
import { CalendarDot } from './EventMarkers';
import { navigateTo } from './navigate';
import { useSettingsStore } from '@/features/settings/settingsStore';

/** The views the switch offers, in the toolbar's own order. */
const VIEWS: ReadonlyArray<{ id: CalendarView; label: string }> = [
  { id: 'month', label: 'Month' },
  { id: 'work_week', label: 'Work week' },
  { id: 'day', label: 'Day' },
  { id: 'agenda', label: 'Agenda' },
];

/** How far each view steps per arrow press. */
function step(view: CalendarView, anchor: string, delta: number): string {
  switch (view) {
    case 'month':
      return addMonths(anchor, delta);
    case 'agenda':
      return addDays(anchor, delta * 7);
    case 'work_week':
      return addDays(anchor, delta * 7);
    case 'day':
      return addDays(anchor, delta);
  }
}

interface CalendarToolbarProps {
  /** Opens the event editor on a new event. */
  onCreate: () => void;
  /** Opens the briefs panel. */
  onBriefs: () => void;
}

/** The calendar manager's rows: dot, name, visibility. */
function CalendarManagerRows() {
  const calendars = useCalendarStore((s) => s.calendars);

  const toggle = (account: string, key: string): void => {
    const without = calendars.filter(
      (calendar) => !(calendar.account === account && calendar.id === key),
    );
    void dispatch(
      intents.setVisibleCalendars(
        without.map((calendar) => ({ account: calendar.account, key: calendar.id })),
      ),
    );
  };

  return (
    <div className="flex flex-col gap-0.5">
      {calendars.map((calendar) => (
        <button
          key={`${calendar.account}:${calendar.id}`}
          type="button"
          aria-label={`Show ${calendar.name}`}
          aria-pressed={calendar.visible}
          onClick={() => toggle(calendar.account, calendar.id)}
          className="flex items-center gap-2 rounded-[var(--radius-xs)] px-2 py-1 text-left text-xs transition-colors hover:bg-hover"
          title={calendar.visible ? `Hide ${calendar.name}` : `Show ${calendar.name}`}
        >
          <CalendarDot color={calendar.color} />
          <span className="min-w-0 flex-1 truncate">{calendar.name}</span>
          <span className="text-muted-foreground">{calendar.visible ? 'shown' : 'hidden'}</span>
        </button>
      ))}
      {calendars.length === 0 && (
        <p className="px-2 py-1 text-xs text-muted-foreground">No calendar yet.</p>
      )}
    </div>
  );
}

/** The toolbar over the calendar views. */
export function CalendarToolbar({ onCreate, onBriefs }: CalendarToolbarProps) {
  const view = useCalendarStore((s) => s.view);
  const anchor = useCalendarStore((s) => s.anchor);
  const nowData = useCalendarStore((s) => s.nowData);
  const setView = useCalendarStore((s) => s.setView);
  const setAnchor = useCalendarStore((s) => s.setAnchor);
  const firstDay = firstDayIndex(useSettingsStore((s) => s.settings.first_day_of_week));
  const [managerOpen, setManagerOpen] = useState(false);
  const managerRef = useRef<HTMLDivElement>(null);

  const zone = nowData?.zone ?? 'Etc/UTC';
  const title = anchor !== null ? monthYearLabel(anchor) : '';

  // A click outside the manager closes it, the dropdown convention.
  useEffect(() => {
    if (!managerOpen) {
      return;
    }
    const close = (event: MouseEvent): void => {
      if (managerRef.current !== null && !managerRef.current.contains(event.target as Node)) {
        setManagerOpen(false);
      }
    };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, [managerOpen]);

  const go = (day: string | null, nextView: CalendarView): void => {
    const target = day ?? anchor;
    if (target === null) {
      return;
    }
    if (nextView !== view) {
      setView(nextView);
    }
    if (day !== null && day !== anchor) {
      setAnchor(day);
    }
    navigateTo(target, nextView, zone, firstDay);
  };

  const today = (): void => {
    if (nowData === null) {
      return;
    }
    go(zoneToday(nowData.now, zone), view);
  };

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-border bg-surface px-3 py-1.5">
      <button
        type="button"
        onClick={today}
        className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs font-medium text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        Today
      </button>
      <div className="flex overflow-hidden rounded-[var(--radius)] border border-border">
        <button
          type="button"
          aria-label="Previous period"
          onClick={() => anchor !== null && go(step(view, anchor, -1), view)}
          className="border-r border-border px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          ‹
        </button>
        <button
          type="button"
          aria-label="Next period"
          onClick={() => anchor !== null && go(step(view, anchor, 1), view)}
          className="px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          ›
        </button>
      </div>
      <h2 className="ml-1 text-sm font-semibold text-foreground">{title}</h2>

      <div className="ml-auto flex items-center gap-2">
        <div className="relative" ref={managerRef}>
          <button
            type="button"
            aria-label="Calendars"
            onClick={() => setManagerOpen((open) => !open)}
            aria-expanded={managerOpen}
            className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Calendars
          </button>
          {managerOpen && (
            <div className="absolute right-0 top-full z-[var(--z-dropdown)] mt-1 w-56 rounded-[var(--radius)] border border-border bg-surface-floating p-1 shadow-[var(--shadow-md)]">
              <CalendarManagerRows />
            </div>
          )}
        </div>
        <button
          type="button"
          onClick={onBriefs}
          className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          Briefs
        </button>
        <div className="flex overflow-hidden rounded-[var(--radius)] border border-border" role="group" aria-label="Calendar view">
          {VIEWS.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              aria-pressed={view === id}
              onClick={() => go(null, id)}
              className={`px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                id === 'month' ? '' : 'border-l border-border'
              } ${
                view === id
                  ? 'bg-selected font-medium text-selected-text'
                  : 'text-muted-foreground hover:bg-hover hover:text-foreground'
              }`}
            >
              {label}
            </button>
          ))}
        </div>
        <button
          type="button"
          onClick={onCreate}
          className="rounded-[var(--radius)] bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground transition-colors hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          New event
        </button>
      </div>
    </div>
  );
}
