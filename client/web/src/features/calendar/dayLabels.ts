// SPDX-License-Identifier: GPL-3.0-only
// The day key labels: one formatting module so every view names a day the
// same way. All formatting runs over the key's UTC-midnight identity with an
// explicit UTC zone, so a label never shifts with the host's own clock.

import { parseDayKey } from './calendarRange';

/** The key's UTC-midnight identity. */
function asUtc(key: string): Date {
  const { year, month, day } = parseDayKey(key);
  return new Date(Date.UTC(year, month - 1, day));
}

/** One localised field of a day key, in the invariant locale. */
function field(key: string, options: Intl.DateTimeFormatOptions): string {
  // The locale inserts a comma after the weekday; the labels read cleaner
  // without it, and one normalisation keeps every view agreeing.
  return new Intl.DateTimeFormat('en-GB', { ...options, timeZone: 'UTC' })
    .format(asUtc(key))
    .replace(', ', ' ');
}

/** `19 October`. */
export function dayMonthLabel(key: string): string {
  return field(key, { day: 'numeric', month: 'long' });
}

/** `Monday 19 October 2026`. */
export function fullDayLabel(key: string): string {
  return field(key, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });
}

/** `Monday 19 October`, the agenda's day header. */
export function agendaDayLabel(key: string): string {
  return field(key, { weekday: 'long', day: 'numeric', month: 'long' });
}

/** `October 2026`, the toolbar's month title. */
export function monthYearLabel(key: string): string {
  return field(key, { month: 'long', year: 'numeric' });
}

/** `Mon`, the grid column headers. */
export function weekdayShortLabel(key: string): string {
  return field(key, { weekday: 'short' });
}

/** The day-of-month number of a key. */
export function dayNumber(key: string): string {
  return String(parseDayKey(key).day);
}

/** Minutes since midnight as `HH:MM`. */
export function clockLabel(minutes: number): string {
  const hour = Math.floor(minutes / 60);
  const minute = Math.floor(minutes % 60);
  return `${String(hour).padStart(2, '0')}:${String(minute).padStart(2, '0')}`;
}
