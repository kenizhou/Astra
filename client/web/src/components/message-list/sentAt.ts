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

/** Formats one row's sent_at stamp for the list; empty when unknown. */
export function formatSentAt(sentAt: string, now: Date): string {
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
      return `${two(stamp.getUTCHours())}:${two(stamp.getUTCMinutes())}`;
    }
    return `${stamp.getUTCDate()} ${month}`;
  }
  return `${stamp.getUTCDate()} ${month} ${stamp.getUTCFullYear()}`;
}
