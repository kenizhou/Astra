// SPDX-License-Identifier: GPL-3.0-only
// One day's placed blocks: the occurrence rows the snapshot holds, split into
// the all-day lane and the timed blocks of the zone-local day, clamped where
// a row runs past either edge, and packed side by side where they overlap so
// nothing renders on top of anything else.

import type { OccurrenceRow } from '@/services/ipc';

import { dayKeyOf } from './calendarRange';

/** One timed occurrence placed in a day, with its overlap packing. */
export interface PlacedBlock {
  /** The occurrence row the block draws. */
  row: OccurrenceRow;
  /** The block's start as minutes since the zone's midnight, clamped to 0. */
  startMinute: number;
  /** The block's end as minutes since the zone's midnight, clamped to 1440. */
  endMinute: number;
  /** Whether the row began on an earlier day. */
  continuesBefore: boolean;
  /** Whether the row runs on into a later day. */
  continuesAfter: boolean;
  /** The overlap column the block sits in, 0-based. */
  column: number;
  /** How many columns the block's overlap cluster spans. */
  columns: number;
}

/** The rows of one civil day: the all-day lane and the timed blocks. */
export interface DayBlocks {
  allDay: OccurrenceRow[];
  timed: PlacedBlock[];
}

/** A row's civil day keys in `zone`, from its resolved instants. */
function spanKeys(row: OccurrenceRow, zone: string): { first: string; last: string } {
  const start = new Date(row.start);
  // An event ending exactly at midnight belongs to the day before it.
  const lastInstant = new Date(new Date(row.end).getTime() - 1);
  return { first: dayKeyOf(start, zone), last: dayKeyOf(lastInstant, zone) };
}

/** The zone-local clock of an instant as minutes since midnight. */
function minutesOf(at: Date, zone: string): number {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone: zone,
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
  }).formatToParts(at);
  const hour = Number(parts.find((part) => part.type === 'hour')?.value ?? 0);
  const minute = Number(parts.find((part) => part.type === 'minute')?.value ?? 0);
  return (hour === 24 ? 0 : hour) * 60 + minute;
}

/**
 * The rows of `dayKey` as the grid lays them out: all-day rows ride the lane
 * once per day they cover, timed rows place by their zone-local clock and
 * clamp where the day cuts them, and overlapping timed blocks pack into
 * side-by-side columns.
 */
export function dayBlocks(
  occurrences: OccurrenceRow[],
  dayKey: string,
  zone: string,
): DayBlocks {
  const allDay: OccurrenceRow[] = [];
  const timed: PlacedBlock[] = [];
  for (const row of occurrences) {
    const { first, last } = spanKeys(row, zone);
    if (dayKey < first || dayKey > last) {
      continue;
    }
    if (row.all_day) {
      allDay.push(row);
      continue;
    }
    const startMinute = first < dayKey ? 0 : minutesOf(new Date(row.start), zone);
    const endMinute = last > dayKey ? 24 * 60 : minutesOf(new Date(row.end), zone);
    timed.push({
      row,
      startMinute,
      endMinute,
      continuesBefore: first < dayKey,
      continuesAfter: last > dayKey,
      column: 0,
      columns: 1,
    });
  }
  timed.sort(
    (a, b) =>
      a.startMinute - b.startMinute || a.endMinute - b.endMinute || a.row.event.localeCompare(b.row.event),
  );
  return { allDay, timed: pack(timed) };
}

/**
 * Packs overlapping blocks into side-by-side columns: a cluster is a run of
 * blocks chained by overlap, each member takes the first column free at its
 * start, and the cluster's width is the most columns any moment needs.
 */
function pack(blocks: PlacedBlock[]): PlacedBlock[] {
  const packed: PlacedBlock[] = [];
  let cluster: PlacedBlock[] = [];
  // The latest end seen so far in the cluster, by column.
  let columnEnds: number[] = [];
  const flush = (): void => {
    const columns = columnEnds.length;
    for (const member of cluster) {
      member.columns = columns;
      packed.push(member);
    }
    cluster = [];
    columnEnds = [];
  };
  for (const block of blocks) {
    if (cluster.length > 0 && block.startMinute >= Math.max(...columnEnds)) {
      flush();
    }
    let column = columnEnds.findIndex((end) => end <= block.startMinute);
    if (column === -1) {
      column = columnEnds.length;
      columnEnds.push(block.endMinute);
    } else {
      columnEnds[column] = block.endMinute;
    }
    block.column = column;
    cluster.push(block);
  }
  flush();
  return packed;
}
