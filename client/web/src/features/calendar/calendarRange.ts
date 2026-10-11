// SPDX-License-Identifier: GPL-3.0-only
// The calendar layout math, pure and zone-aware: instants localise into the
// snapshot's own display zone through Intl rather than the host's, day keys
// stay civil dates so daylight saving never moves a date, and the drag
// snapping mirrors the bound the core's drag arithmetic enforces. Layout
// math only: no store, no React, no dispatch.

/** The slot the grids snap drags to, in minutes. */
export const SLOT_MINUTES = 15;

/**
 * The furthest a single drag may carry an event, in days. The mirror of the
 * core's own bound: a gesture past it never crosses the IPC.
 */
export const MAX_DRAG_DAYS = 366;

/** Minutes in a civil day. */
const DAY_MINUTES = 24 * 60;

/** A zoned wall clock's date fields as the display zone reads them. */
export interface ZoneParts {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  weekday: number;
}

/** Two-digit padding for date fields. */
function pad(value: number): string {
  return String(value).padStart(2, '0');
}

/** The parts of `at` as `zone` reads them, through a cached formatter. */
export function zonedParts(at: Date, zone: string): ZoneParts {
  const dtf = new Intl.DateTimeFormat('en-US', {
    timeZone: zone,
    hour12: false,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    weekday: 'short',
  });
  const parts = dtf.formatToParts(at);
  const read = (type: string): string =>
    parts.find((part) => part.type === type)?.value ?? '';
  const weekdays = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const hour = Number(read('hour'));
  return {
    year: Number(read('year')),
    month: Number(read('month')),
    day: Number(read('day')),
    // A midnight hour formats as 24 under some ICU versions; normalise.
    hour: hour === 24 ? 0 : hour,
    minute: Number(read('minute')),
    weekday: weekdays.indexOf(read('weekday')),
  };
}

/** The civil day key (`YYYY-MM-DD`) of `at` as `zone` reads it. */
export function dayKeyOf(at: Date, zone: string): string {
  const parts = zonedParts(at, zone);
  return `${parts.year}-${pad(parts.month)}-${pad(parts.day)}`;
}

/** The zone-local clock of `at` as minutes since the zone's midnight. */
export function minutesInZone(at: Date, zone: string): number {
  const parts = zonedParts(at, zone);
  return parts.hour * 60 + parts.minute;
}

/** The zone-local today of a snapshot's now instant. */
export function zoneToday(now: string, zone: string): string {
  return dayKeyOf(new Date(now), zone);
}

/** A day key's civil date fields. */
export function parseDayKey(key: string): { year: number; month: number; day: number } {
  const [year, month, day] = key.split('-').map(Number);
  return { year: year!, month: month!, day: day! };
}

/** A day key as a UTC-midnight Date: a civil identity, never a zone instant. */
function keyAsUtc(key: string): Date {
  const { year, month, day } = parseDayKey(key);
  return new Date(Date.UTC(year, month - 1, day));
}

/** The day key of a UTC-midnight civil identity. */
function keyOfUtc(at: Date): string {
  return `${at.getUTCFullYear()}-${pad(at.getUTCMonth() + 1)}-${pad(at.getUTCDate())}`;
}

/** `key` moved by whole civil days. */
export function addDays(key: string, days: number): string {
  const at = keyAsUtc(key);
  at.setUTCDate(at.getUTCDate() + days);
  return keyOfUtc(at);
}

/** `key` moved by whole months, clamping the day when the target is shorter. */
export function addMonths(key: string, months: number): string {
  const { day } = parseDayKey(key);
  const first = keyAsUtc(key);
  first.setUTCDate(1);
  first.setUTCMonth(first.getUTCMonth() + months);
  const lastDay = new Date(
    Date.UTC(first.getUTCFullYear(), first.getUTCMonth() + 1, 0),
  ).getUTCDate();
  first.setUTCDate(Math.min(day, lastDay));
  return keyOfUtc(first);
}

/** The weekday of a day key as a JS index, Sunday 0 to Saturday 6. */
export function weekdayOfKey(key: string): number {
  return keyAsUtc(key).getUTCDay();
}

/** The Monday a day key's week opens at. */
export function weekStartOfKey(key: string): string {
  return addDays(key, -((weekdayOfKey(key) + 6) % 7));
}

/** The seven civil day keys of the Monday-start week holding `anchor`. */
export function weekOf(anchor: string): string[] {
  const start = weekStartOfKey(anchor);
  return Array.from({ length: 7 }, (_, i) => addDays(start, i));
}

/** The five weekday keys of the work week holding `anchor`. */
export function workWeekOf(anchor: string): string[] {
  return weekOf(anchor).slice(0, 5);
}

/**
 * The fixed six-week month grid the month view lays out: the Monday before
 * the anchor month's first day, 42 civil keys.
 */
export function monthGrid(anchor: string): string[] {
  const { year, month } = parseDayKey(anchor);
  const first = `${year}-${pad(month)}-01`;
  const start = weekStartOfKey(first);
  return Array.from({ length: 42 }, (_, i) => addDays(start, i));
}

/** A UTC instant string (`…Z`) from a Date, whole seconds. */
function toUtcString(at: Date): string {
  return (
    `${at.getUTCFullYear()}-${pad(at.getUTCMonth() + 1)}-${pad(at.getUTCDate())}` +
    `T${pad(at.getUTCHours())}:${pad(at.getUTCMinutes())}:${pad(at.getUTCSeconds())}Z`
  );
}

/** The zone's offset from UTC at `at`, in minutes, east positive. */
function zoneOffsetMinutes(at: Date, zone: string): number {
  const parts = zonedParts(at, zone);
  const wallAsUtc = Date.UTC(
    parts.year,
    parts.month - 1,
    parts.day,
    parts.hour,
    parts.minute,
  );
  return Math.round((wallAsUtc - at.getTime()) / 60_000);
}

/**
 * A zone wall clock resolved to its UTC instant string: the offset probed
 * twice so a guess that lands on the far side of a daylight-saving change
 * still resolves through the offset that governs the wall clock itself.
 */
export function instantOfZoned(key: string, minutes: number, zone: string): string {
  const { year, month, day } = parseDayKey(key);
  const wall = Date.UTC(year, month - 1, day, 0, minutes);
  const first = zoneOffsetMinutes(new Date(wall), zone);
  const governing = zoneOffsetMinutes(new Date(wall - first * 60_000), zone);
  return toUtcString(new Date(wall - governing * 60_000));
}

/** Minutes snapped down onto the slot grid. */
export function snapToSlot(minutes: number, slot: number = SLOT_MINUTES): number {
  return Math.floor(minutes / slot) * slot;
}

/** One position a pointer holds on the grid: a day column and a clock. */
export interface GridPosition {
  day: number;
  minutes: number;
}

/**
 * The drag delta between two grid positions, both ends snapped to the slot
 * grid: the `{days, minutes}` the core's move intent takes. `null` when the
 * span escapes the 366-day bound, so the gesture never crosses the IPC.
 */
export function dragFromGrid(
  from: GridPosition,
  to: GridPosition,
): { days: number; minutes: number } | null {
  const days = to.day - from.day;
  const minutes = snapToSlot(to.minutes) - snapToSlot(from.minutes);
  const span = days + Math.trunc(minutes / DAY_MINUTES);
  if (Math.abs(span) > MAX_DRAG_DAYS) {
    return null;
  }
  return { days, minutes };
}
