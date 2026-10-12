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
export function formatSentAt(sentAt: string, now: Date, twentyFour = true): string {
  if (sentAt === '') {
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
