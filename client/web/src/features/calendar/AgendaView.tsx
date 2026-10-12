// SPDX-License-Identifier: GPL-3.0-only
// The agenda view: the snapshot's occurrences grouped under their zone-local
// day headers and laid out chronologically, one row per occurrence with its
// clock under the clock setting, its calendar's colour, and its markers; a
// hold draws dotted the same as every other view.

import { useMemo } from 'react';

import type { OccurrenceRow } from '@/services/ipc';

import { useCalendarStore } from './calendarStore';
import { dayKeyOf } from './calendarRange';
import { agendaDayLabel, clockLabel } from './dayLabels';
import { CalendarDot, RepeatGlyph, holdClass, isHold } from './EventMarkers';
import { minutesInZone } from './calendarRange';
import { useZonedNow } from './useZonedNow';
import { useSettingsStore } from '@/features/settings/settingsStore';

/** One agenda row. */
function Row({
  row,
  zone,
  color,
  onOpenEvent,
}: {
  row: OccurrenceRow;
  zone: string;
  color: string | undefined;
  onOpenEvent: (row: OccurrenceRow) => void;
}) {
  const twentyFour = useSettingsStore((s) => s.settings.twenty_four_clock);
  const hold = isHold(row);
  const title = row.title === '' ? '(no title)' : row.title;
  return (
    <li
      data-hold={hold}
      className={`flex min-w-0 items-center gap-2 rounded-[var(--radius-xs)] border border-transparent px-2 py-1.5 text-[13px] transition-colors hover:bg-hover ${holdClass(hold)}`}
      style={hold ? { borderColor: color ?? 'var(--primary)' } : undefined}
    >
      <span className="w-16 shrink-0 text-right text-xs tabular-nums text-muted-foreground">
        {row.all_day ? (
          <span className="rounded bg-accent px-1 py-0.5 text-[10px] text-accent-foreground">
            All day
          </span>
        ) : (
          clockLabel(minutesInZone(new Date(row.start), zone), twentyFour)
        )}
      </span>
      <CalendarDot color={color} />
      {row.recurring && (
        <span title={`${title} repeats`} className="flex shrink-0 items-center text-muted-foreground">
          <RepeatGlyph />
        </span>
      )}
      <button
        type="button"
        onClick={() => onOpenEvent(row)}
        className={`min-w-0 flex-1 truncate text-left ${hold ? 'text-muted-foreground' : 'text-foreground'}`}
        title={hold ? `${title} awaits your answer` : title}
      >
        {title}
      </button>
    </li>
  );
}

/** The chronological day-grouped list over the whole window. */
export function AgendaView({ onOpenEvent }: { onOpenEvent: (row: OccurrenceRow) => void }) {
  const occurrences = useCalendarStore((s) => s.occurrences);
  const calendars = useCalendarStore((s) => s.calendars);
  const nowData = useCalendarStore((s) => s.nowData);
  const zone = nowData?.zone ?? 'Etc/UTC';
  const now = useZonedNow(nowData);
  const today = now !== null ? dayKeyOf(now, zone) : null;

  const byDay = useMemo(() => {
    const days = new Map<string, OccurrenceRow[]>();
    for (const row of [...occurrences].sort((a, b) => a.start.localeCompare(b.start))) {
      const key = dayKeyOf(new Date(row.start), zone);
      const bucket = days.get(key);
      if (bucket !== undefined) {
        bucket.push(row);
      } else {
        days.set(key, [row]);
      }
    }
    return [...days.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [occurrences, zone]);

  const colorOf = (row: OccurrenceRow): string | undefined =>
    calendars.find((c) => c.account === row.account && c.id === row.calendar)?.color;

  if (byDay.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 p-8 text-sm text-muted-foreground">
        No events in this window.
      </div>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto p-3">
      {byDay.map(([dayKey, rows]) => (
        <section key={dayKey} className="mb-3">
          <h3
            className={`mb-1 px-1 text-xs font-medium ${
              dayKey === today ? 'text-primary' : 'text-muted-foreground'
            }`}
          >
            {agendaDayLabel(dayKey)}
          </h3>
          <ul className="space-y-1">
            {rows.map((row) => (
              <Row
                key={`${row.account}:${row.event}:${row.start}`}
                row={row}
                zone={zone}
                color={colorOf(row)}
                onOpenEvent={onOpenEvent}
              />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
