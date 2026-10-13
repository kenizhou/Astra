// SPDX-License-Identifier: GPL-3.0-only
// The list's sent-at stamp. The wire instant is UTC and truncated to the
// whole second, so the label derives from the UTC parts alone: hours and
// minutes within the current UTC day, day and month within the current UTC
// year, the year added only when it differs.

/** The month abbreviations the label uses, in UTC order. */
const MONTHS = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
] as const;

/** Formats one row's sent_at stamp for the list; empty when unknown. The
 * day's clock label honours the 24-hour setting so the list and the reading
 * pane spell one time the same way. */
export function formatSentAt(sentAt: string, now: Date, twentyFour = true): string {  if (sentAt === '') {
    return '';
  }
  const stamp = new Date(sentAt);
  if (Number.isNaN(stamp.getTime())) {
    return '';
  }
  const two = (value: number): string => String(value).padStart(2, '0');
  const month = MONTHS[stamp.getUTCMonth()] ?? '';
  if (stamp.getUTCFullYear() === now.getUTCFullYear()) {
    if (stamp.getUTCMonth() === now.getUTCMonth() && stamp.getUTCDate() === now.getUTCDate()) {
      const hour = stamp.getUTCHours();
      if (twentyFour) {
        return `${two(hour)}:${two(stamp.getUTCMinutes())}`;
      }
      const meridiem = hour < 12 ? 'AM' : 'PM';
      const twelve = hour % 12 === 0 ? 12 : hour % 12;
      return `${twelve}:${two(stamp.getUTCMinutes())} ${meridiem}`;
    }
    return `${stamp.getUTCDate()} ${month}`;
  }
  return `${stamp.getUTCDate()} ${month} ${stamp.getUTCFullYear()}`;
}

/** The weekday names the group headers use, in UTC order. */
const WEEKDAYS = [
  'Sunday',
  'Monday',
  'Tuesday',
  'Wednesday',
  'Thursday',
  'Friday',
  'Saturday',
] as const;

const DAY_MS = 86_400_000;

/** The UTC midnight one instant falls on. */
function utcDayStart(stamp: Date): number {
  return Date.UTC(stamp.getUTCFullYear(), stamp.getUTCMonth(), stamp.getUTCDate());
}

/** The date group header one row's sent_at falls under, against the clock the
 * list renders with. Days compare in UTC, exactly as the row stamps do:
 * Today, Yesterday, the weekday within the last week, and the date beyond it.
 * A row whose stamp is unknown groups under Earlier. */
export function sentDayLabel(sentAt: string, now: Date): string {
  const stamp = new Date(sentAt);
  if (sentAt === '' || Number.isNaN(stamp.getTime())) {
    return 'Earlier';
  }
  const start = utcDayStart(stamp);
  const todayStart = utcDayStart(now);
  if (start >= todayStart) {
    return 'Today';
  }
  if (start >= todayStart - DAY_MS) {
    return 'Yesterday';
  }
  if (start >= todayStart - 6 * DAY_MS) {
    return WEEKDAYS[stamp.getUTCDay()] ?? 'Earlier';
  }
  const month = MONTHS[stamp.getUTCMonth()] ?? '';
  if (stamp.getUTCFullYear() === now.getUTCFullYear()) {
    return `${stamp.getUTCDate()} ${month}`;
  }
  return `${stamp.getUTCDate()} ${month} ${stamp.getUTCFullYear()}`;
}
