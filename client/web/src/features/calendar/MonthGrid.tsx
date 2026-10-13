// SPDX-License-Identifier: GPL-3.0-only
// The month view: the fixed six-week civil grid around the anchor month
// under a weekday header row in the grid's own first-day order, a day cell
// per key with its chips bucketed by zone-local day, the overflow affordance
// into the day view, and today named from the snapshot's now data.

import { useCalendarStore } from './calendarStore';
import { useSettingsStore } from '@/features/settings/settingsStore';
import { dayKeyOf, monthGrid, parseDayKey } from './calendarRange';
import { fullDayLabel, firstDayIndex, weekdayShortLabel } from './dayLabels';
import { CalendarDot, RepeatGlyph, holdClass, isHold } from './EventMarkers';
import { dayBlocks } from './rowLayout';
import type { OccurrenceRow } from '@/services/ipc';

/** How many chips a month cell shows before offering the day view. */
const MAX_CHIPS = 3;

/** One chip of a month cell. */
function Chip({
  row,
  color,
  onOpenEvent,
}: {
  row: OccurrenceRow;
  color: string | undefined;
  onOpenEvent: (row: OccurrenceRow) => void;
}) {
  const hold = isHold(row);
  const title = row.title === '' ? '(no title)' : row.title;
  return (
    <button
      type="button"
      data-chip=""
      data-hold={hold}
      title={title}
      onClick={() => onOpenEvent(row)}
      className={`flex w-full items-center gap-1 truncate rounded-[var(--radius)] border px-1 text-left text-[10px] leading-4 ${holdClass(hold)}`}
      style={
        hold
          ? { borderColor: color ?? 'var(--primary)' }
          : {
              backgroundColor: `color-mix(in srgb, ${color ?? 'var(--primary)'} 12%, var(--background))`,
              borderColor: color ?? 'var(--primary)',
            }
      }
    >
      {row.recurring && (
        <span title={`${title} repeats`} className="flex shrink-0 items-center">
          <RepeatGlyph />
        </span>
      )}
      <CalendarDot color={color} />
      <span className="truncate">{title}</span>
    </button>
  );
}

/** The month grid over the anchor month. */
export function MonthGrid({
  onPickDay,
  onOpenEvent,
}: {
  /** Navigates the anchor to a day (the day header and the overflow). */
  onPickDay: (dayKey: string) => void;
  /** Opens an event's editor from a chip. */
  onOpenEvent: (row: OccurrenceRow) => void;
}) {
  const anchor = useCalendarStore((s) => s.anchor);
  const occurrences = useCalendarStore((s) => s.occurrences);
  const calendars = useCalendarStore((s) => s.calendars);
  const nowData = useCalendarStore((s) => s.nowData);
  const firstDay = firstDayIndex(useSettingsStore((s) => s.settings.first_day_of_week));

  if (anchor === null) {
    return <p className="p-4 text-sm text-muted-foreground">The calendar has no clock yet.</p>;
  }
  const zone = nowData?.zone ?? 'Etc/UTC';
  const today = nowData !== null ? dayKeyOf(new Date(nowData.now), zone) : null;
  const anchorMonth = parseDayKey(anchor).month;
  const grid = monthGrid(anchor, firstDay);
  const colorOf = (row: OccurrenceRow): string | undefined =>
    calendars.find((c) => c.account === row.account && c.id === row.calendar)?.color;

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="grid shrink-0 grid-cols-7 border-b border-border-subtle bg-surface">
        {grid.slice(0, 7).map((dayKey) => (
          <div
            key={dayKey}
            className="px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-text"
          >
            {weekdayShortLabel(dayKey)}
          </div>
        ))}
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-7 overflow-y-auto border-l border-border-subtle">
      {grid.map((dayKey) => {
        const inMonth = parseDayKey(dayKey).month === anchorMonth;
        const blocks = dayBlocks(occurrences, dayKey, zone);
        const chips = [...blocks.allDay, ...blocks.timed.map((block) => block.row)];
        const overflow = chips.length - MAX_CHIPS;
        const isToday = today === dayKey;
        return (
          <div
            key={dayKey}
            aria-label={fullDayLabel(dayKey)}
            data-in-month={inMonth}
            data-today={isToday}
            className={`flex min-h-[92px] flex-col gap-0.5 border-b border-r border-border-subtle p-1 transition-colors ${
              inMonth ? 'hover:bg-hover' : 'bg-surface'
            }`}
          >
            <button
              type="button"
              aria-label={`Go to ${fullDayLabel(dayKey)}`}
              onClick={() => onPickDay(dayKey)}
              className={`flex h-5 w-5 items-center justify-center self-end rounded-full text-[11px] tabular-nums transition-colors hover:bg-hover ${
                isToday
                  ? 'bg-primary font-semibold text-primary-foreground'
                  : inMonth
                    ? 'text-foreground'
                    : 'text-muted-foreground'
              }`}
            >
              {parseDayKey(dayKey).day}
            </button>
            <div className="flex min-h-0 flex-col gap-0.5 overflow-hidden">
              {chips.slice(0, MAX_CHIPS).map((row) => (
                <Chip
                  key={`${row.account}:${row.event}:${row.start}`}
                  row={row}
                  color={colorOf(row)}
                  onOpenEvent={onOpenEvent}
                />
              ))}
              {overflow > 0 && (
                <button
                  type="button"
                  onClick={() => onPickDay(dayKey)}
                  className="rounded px-1 py-0.5 text-left text-[10px] font-medium text-muted-foreground transition-colors hover:bg-hover hover:text-foreground"
                >
                  {overflow} more on {fullDayLabel(dayKey)}
                </button>
              )}
            </div>
          </div>
        );
      })}
      </div>
    </div>
  );
}
