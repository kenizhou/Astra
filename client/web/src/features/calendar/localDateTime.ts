// SPDX-License-Identifier: GPL-3.0-only
// The event dialog's datetime-local helpers: the input value of an instant
// or a civil slot in the display zone, and the wall-clock deltas a time
// edit crosses the move intent in. Pure conversions over calendarRange.

import { dayKeyOf, minutesInZone } from './calendarRange';

/** A datetime-local value as a civil day and minute. */
export function parseLocal(text: string): { day: string; minutes: number } {
  const [date, clock] = text.split('T');
  const [hour, minute] = (clock ?? '00:00').split(':').map(Number);
  return { day: date!, minutes: hour! * 60 + minute! };
}

/** A civil day as its UTC day number, for day deltas. */
function dayNumber(day: string): number {
  const [year, month, date] = day.split('-').map(Number);
  return Math.floor(Date.UTC(year!, month! - 1, date!) / 86_400_000);
}

/** The wall-clock delta between two local values, unsnapped. */
export function localDelta(from: string, to: string): { days: number; minutes: number } {
  const a = parseLocal(from);
  const b = parseLocal(to);
  return { days: dayNumber(b.day) - dayNumber(a.day), minutes: b.minutes - a.minutes };
}

/** A zone-local clock of an instant as a datetime-local value. */
export function localValueOf(instant: string, zone: string): string {
  const at = new Date(instant);
  return localSlotValue(dayKeyOf(at, zone), minutesInZone(at, zone));
}

/** A civil day and minute as a datetime-local value. */
export function localSlotValue(day: string, minutes: number): string {
  const hh = String(Math.floor(minutes / 60)).padStart(2, '0');
  const mm = String(minutes % 60).padStart(2, '0');
  return `${day}T${hh}:${mm}`;
}
