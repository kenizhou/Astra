// SPDX-License-Identifier: GPL-3.0-only
// The mini-month navigator: a compact month grid bound to the navigation
// anchor. A day click moves the anchor (and the core's window with it)
// without leaving the active view; the chevrons browse the mini grid's own
// month, which follows the anchor's month whenever navigation lands in
// another. The anchored day and today are marked, today with the month
// view's primary pill.

import { useState } from 'react';

import { ChevronLeftIcon, ChevronRightIcon } from '@/components/icons';
import { useSettingsStore } from '@/features/settings/settingsStore';

import { addMonths, dayKeyOf, monthGrid, parseDayKey } from './calendarRange';
import { monthYearLabel, firstDayIndex, weekdayShortLabel } from './dayLabels';
import { navigateTo } from './navigate';
import { useCalendarStore } from './calendarStore';

/** The first of the month a day key falls in. */
function monthStart(key: string): string {
  const { year, month } = parseDayKey(key);
  return `${year}-${String(month).padStart(2, '0')}-01`;
}

/** The mini-month navigator over the anchor's month. */
export function MiniMonth() {
  const anchor = useCalendarStore((s) => s.anchor);
  const view = useCalendarStore((s) => s.view);
  const nowData = useCalendarStore((s) => s.nowData);
  const setAnchor = useCalendarStore((s) => s.setAnchor);
  const firstDay = firstDayIndex(useSettingsStore((s) => s.settings.first_day_of_week));

  const zone = nowData?.zone ?? 'Etc/UTC';
  const today = nowData !== null ? dayKeyOf(new Date(nowData.now), zone) : null;

  // The browsed month follows the anchor's month, while the user's own
  // chevron browsing stands until navigation lands elsewhere.
  const [browsed, setBrowsed] = useState(() => monthStart(anchor ?? '1970-01-01'));
  const [prevAnchorMonth, setPrevAnchorMonth] = useState(() => monthStart(anchor ?? '1970-01-01'));
  if (anchor !== null && monthStart(anchor) !== prevAnchorMonth) {
    setPrevAnchorMonth(monthStart(anchor));
    setBrowsed(monthStart(anchor));
  }

  if (anchor === null) {
    return null;
  }

  const cells = monthGrid(browsed, firstDay);
  const browsedMonth = parseDayKey(browsed).month;

  const pick = (dayKey: string): void => {
    setAnchor(dayKey);
    navigateTo(dayKey, view, zone, firstDay);
  };

  return (
    <div aria-label="Mini month" className="shrink-0 border-b border-border-subtle px-2 py-2">
      <div className="flex items-center gap-0.5">
        <button
          type="button"
          aria-label="Previous month"
          onClick={() => setBrowsed((month) => addMonths(month, -1))}
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded-[var(--radius-xs)] text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <ChevronLeftIcon size={12} />
        </button>
        <span className="flex-1 truncate text-center text-xs font-semibold text-foreground">
          {monthYearLabel(browsed)}
        </span>
        <button
          type="button"
          aria-label="Next month"
          onClick={() => setBrowsed((month) => addMonths(month, 1))}
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded-[var(--radius-xs)] text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <ChevronRightIcon size={12} />
        </button>
      </div>
      <div className="grid grid-cols-7 pt-1.5">
        {cells.slice(0, 7).map((dayKey) => (
          <span
            key={dayKey}
            className="pb-1 text-center text-[10px] font-medium uppercase text-muted-text"
          >
            {weekdayShortLabel(dayKey).slice(0, 2)}
          </span>
        ))}
        {cells.map((dayKey) => {
          const inMonth = parseDayKey(dayKey).month === browsedMonth;
          const isToday = dayKey === today;
          const isAnchor = dayKey === anchor;
          return (
            <button
              key={dayKey}
              type="button"
              aria-label={`Go to ${dayKey}`}
              aria-pressed={isAnchor}
              onClick={() => pick(dayKey)}
              className={`mx-auto flex h-6 w-6 items-center justify-center rounded-full text-[11px] tabular-nums transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                isToday
                  ? 'bg-primary font-semibold text-primary-foreground'
                  : isAnchor
                    ? 'bg-primary-muted font-medium text-foreground'
                    : inMonth
                      ? 'text-foreground'
                      : 'text-muted-foreground'
              }`}
            >
              {parseDayKey(dayKey).day}
            </button>
          );
        })}
      </div>
    </div>
  );
}
