// SPDX-License-Identifier: GPL-3.0-only
// The shared time grid: an hour gutter, one column per civil day with an
// all-day lane above, blocks placed by their zone-local clock and packed
// where they overlap, the now line advanced by the host clock, and the drag
// gesture bound over the columns. The day and work-week grids are this grid
// over different day sets.

import type { OccurrenceRow } from '@/services/ipc';

import { useCalendarStore } from './calendarStore';
import { dayKeyOf, minutesInZone } from './calendarRange';
import {
  DAY_H,
  GUTTER_W,
  HOUR_H,
  minuteTop,
} from './gridGeometry';
import {
  CalendarDot,
  RepeatGlyph,
  holdClass,
  isHold,
} from './EventMarkers';
import { dayBlocks, type PlacedBlock } from './rowLayout';
import {
  clockLabel,
  dayNumber,
  fullDayLabel,
  weekdayShortLabel,
} from './dayLabels';
import { useEventDrag } from './useEventDrag';
import { useZonedNow } from './useZonedNow';

interface TimeGridProps {
  /** The civil day keys the grid lays out, Monday-first for multi-day sets. */
  days: string[];
  /** The grid's accessible name, naming the span it shows. */
  ariaLabel: string;
  /** Opens an event's editor. */
  onOpenEvent: (row: OccurrenceRow) => void;
}

/** One placed block of a day column. */
function Block({
  block,
  calendarColor,
  ghostShift,
  onPointerDown,
  onClick,
}: {
  block: PlacedBlock;
  calendarColor: string | undefined;
  ghostShift: number;
  onPointerDown: (event: React.PointerEvent) => void;
  onClick: () => void;
}) {
  const hold = isHold(block.row);
  const width = 100 / block.columns;
  return (
    <div
      data-block=""
      data-hold={hold}
      role="button"
      tabIndex={0}
      title={block.row.title === '' ? '(no title)' : block.row.title}
      onPointerDown={onPointerDown}
      onClick={onClick}
      style={{
        position: 'absolute',
        top: minuteTop(block.startMinute),
        height: Math.max(minuteTop(block.endMinute - block.startMinute), 16),
        left: `${block.column * width}%`,
        width: `${width}%`,
        transform: ghostShift === 0 ? undefined : `translateY(${ghostShift}px)`,
        ...colorStyling(calendarColor, hold),
      }}
      className={`overflow-hidden rounded-[var(--radius-xs)] border px-1 py-0.5 text-left text-[11px] leading-tight transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${holdClass(hold)} ${
        block.row.can_move ? 'cursor-grab' : 'cursor-pointer'
      }`}
    >
      <span className="flex items-center gap-1 font-medium">
        {block.row.recurring && (
          <span title={`${block.row.title || '(no title)'} repeats`} className="flex items-center">
            <RepeatGlyph />
          </span>
        )}
        <CalendarDot color={calendarColor} />
        <span className="truncate">{block.row.title === '' ? '(no title)' : block.row.title}</span>
      </span>
      {(block.endMinute - block.startMinute >= 45 || block.continuesBefore || block.continuesAfter) && (
        <span className="block truncate text-[10px] text-muted-foreground">
          {clockLabel(block.startMinute)} to {clockLabel(block.endMinute)}
        </span>
      )}
    </div>
  );
}

/** The colour styling for a block from its calendar's colour. */
function colorStyling(color: string | undefined, hold: boolean) {
  return hold
    ? { borderColor: color ?? 'var(--primary)' }
    : {
        backgroundColor: `color-mix(in srgb, ${color ?? 'var(--primary)'} 14%, var(--background))`,
        borderColor: color ?? 'var(--primary)',
      };
}

/** One day column of the grid. */
function DayColumn({
  dayKey,
  zone,
  occurrences,
  colorOf,
  dragging,
  beginDrag,
  consumeClick,
  onOpenEvent,
  now,
  nowDay,
}: {
  dayKey: string;
  zone: string;
  occurrences: OccurrenceRow[];
  colorOf: (row: OccurrenceRow) => string | undefined;
  dragging: ReturnType<typeof useEventDrag>['dragging'];
  beginDrag: ReturnType<typeof useEventDrag>['beginDrag'];
  consumeClick: () => boolean;
  onOpenEvent: (row: OccurrenceRow) => void;
  now: Date | null;
  nowDay: string | null;
}) {
  const blocks = dayBlocks(occurrences, dayKey, zone);
  return (
    <div
      aria-label={`Day column, ${fullDayLabel(dayKey)}`}
      className="relative min-w-0 flex-1 border-l border-border"
    >
      {Array.from({ length: 24 }, (_, hour) => (
        <div key={hour} style={{ height: HOUR_H }} className="border-b border-border/40" />
      ))}
      {blocks.timed.map((block) => (
        <Block
          key={`${block.row.account}:${block.row.event}:${block.row.start}`}
          block={block}
          calendarColor={colorOf(block.row)}
          ghostShift={
            dragging !== null && dragging.row === block.row
              ? minuteTop(
                  Math.round((dragging.to.minutes - dragging.from.minutes) * 60) / 60,
                )
              : 0
          }
          onPointerDown={(event) => beginDrag(block.row, event)}
          onClick={() => {
            if (!consumeClick()) {
              onOpenEvent(block.row);
            }
          }}
        />
      ))}
      {now !== null && nowDay === dayKey && (
        <div
          aria-label="Now"
          className="pointer-events-none absolute left-0 right-0 z-10 border-t-2 border-[var(--destructive)]"
          style={{ top: minuteTop(minutesInZone(now, zone)) }}
        >
          <span className="absolute -left-1 -top-[3px] h-1.5 w-1.5 rounded-full bg-[var(--destructive)]" />
        </div>
      )}
    </div>
  );
}

/** The shared hour grid over the given civil days. */
export function TimeGrid({ days, ariaLabel, onOpenEvent }: TimeGridProps) {
  const occurrences = useCalendarStore((s) => s.occurrences);
  const calendars = useCalendarStore((s) => s.calendars);
  const nowData = useCalendarStore((s) => s.nowData);
  const zone = nowData?.zone ?? 'Etc/UTC';
  const now = useZonedNow(nowData !== null ? zone : null);
  const nowDay = now !== null ? dayKeyOf(now, zone) : null;
  const drag = useEventDrag(days.length);

  const colorOf = (row: OccurrenceRow): string | undefined =>
    calendars.find(
      (calendar) => calendar.account === row.account && calendar.id === row.calendar,
    )?.color;

  return (
    <section className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex shrink-0 border-b border-border bg-surface">
        <div style={{ width: GUTTER_W }} className="shrink-0" />
        {days.map((dayKey) => (
          <div
            key={dayKey}
            className={`min-w-0 flex-1 border-l border-border px-2 py-1 text-xs ${
              nowDay === dayKey ? 'font-semibold text-primary' : 'text-muted-foreground'
            }`}
          >
            <span className="mr-1">{weekdayShortLabel(dayKey)}</span>
            <span className="tabular-nums">{dayNumber(dayKey)}</span>
          </div>
        ))}
      </div>
      <div className="flex shrink-0 border-b border-border bg-surface">
        <div
          style={{ width: GUTTER_W }}
          className="shrink-0 px-2 py-1 text-[10px] uppercase tracking-wide text-muted-foreground"
        >
          All day
        </div>
        {days.map((dayKey) => (
          <div
            key={dayKey}
            aria-label={`All day, ${fullDayLabel(dayKey)}`}
            className="flex min-h-[24px] min-w-0 flex-1 flex-col gap-0.5 border-l border-border px-1 py-0.5"
          >
            {dayBlocks(occurrences, dayKey, zone).allDay.map((row) => (
              <button
                key={`${row.account}:${row.event}`}
                type="button"
                data-hold={isHold(row)}
                onClick={() => onOpenEvent(row)}
                title={row.title === '' ? '(no title)' : row.title}
                className={`flex items-center gap-1 truncate rounded-[var(--radius-xs)] border px-1 text-left text-[11px] ${holdClass(isHold(row))}`}
                style={colorStyling(colorOf(row), isHold(row))}
              >
                {row.recurring && (
                  <span title={`${row.title || '(no title)'} repeats`}>
                    <RepeatGlyph />
                  </span>
                )}
                <span className="truncate">{row.title === '' ? '(no title)' : row.title}</span>
              </button>
            ))}
          </div>
        ))}
      </div>
      <div
        ref={drag.gridRef}
        aria-label={ariaLabel}
        className="flex min-h-0 flex-1 overflow-y-auto"
        {...drag.handlers}
        style={{ height: DAY_H }}
      >
        <div style={{ width: GUTTER_W }} className="shrink-0">
          {Array.from({ length: 24 }, (_, hour) => (
            <div
              key={hour}
              style={{ height: HOUR_H }}
              className="relative border-b border-border/40 text-right text-[10px] text-muted-foreground"
            >
              <span className="absolute -top-1.5 right-1">{clockLabel(hour * 60)}</span>
            </div>
          ))}
        </div>
        {days.map((dayKey) => (
          <DayColumn
            key={dayKey}
            dayKey={dayKey}
            zone={zone}
            occurrences={occurrences}
            colorOf={colorOf}
            dragging={drag.dragging}
            beginDrag={drag.beginDrag}
            consumeClick={drag.consumeClick}
            onOpenEvent={onOpenEvent}
            now={now}
            nowDay={nowDay}
          />
        ))}
      </div>
    </section>
  );
}
