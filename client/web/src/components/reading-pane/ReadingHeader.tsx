// SPDX-License-Identifier: GPL-3.0-only
// The reading header: subject, the sender avatar beside the name, and the
// full UTC stamp, all straight from the snapshot. The avatar resolves
// through the people command on the sender's address, the one resolution
// every surface shares. The stamp derives from the UTC parts the same way
// the list's does, with the year always shown and the time of day appended.

import { Avatar } from '@/features/people/Avatar';

/** The month abbreviations the stamp uses, in UTC order. */
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

/** Formats the open message's sent_at as `9 Oct 2026, 08:30`; empty when unknown. */
export function formatReadingDate(sentAt: string): string {
  if (sentAt === '') {
    return '';
  }
  const stamp = new Date(sentAt);
  if (Number.isNaN(stamp.getTime())) {
    return '';
  }
  const two = (value: number): string => String(value).padStart(2, '0');
  const month = MONTHS[stamp.getUTCMonth()] ?? '';
  return `${stamp.getUTCDate()} ${month} ${stamp.getUTCFullYear()}, ${two(stamp.getUTCHours())}:${two(stamp.getUTCMinutes())}`;
}

interface ReadingHeaderProps {
  /** The subject, empty if none. */
  subject: string;
  /** The sender's display name, empty when the header carried none. */
  fromName: string;
  /** The sender's bare email address, empty when there is no sender. */
  fromAddress: string;
  /** When the message was sent, as a whole-second UTC instant string. */
  sentAt: string;
}

export function ReadingHeader({ subject, fromName, fromAddress, sentAt }: ReadingHeaderProps) {
  const from =
    fromName === '' ? fromAddress : fromAddress === '' ? fromName : `${fromName} <${fromAddress}>`;
  const date = formatReadingDate(sentAt);
  return (
    <div className="shrink-0 border-b border-border px-4 py-3">
      <h2 className="truncate text-sm font-semibold text-foreground">
        {subject === '' ? '(no subject)' : subject}
      </h2>
      <div className="mt-1 flex items-center gap-2">
        {fromAddress !== '' && <Avatar address={fromAddress} size={28} />}
        {from !== '' && <p className="min-w-0 truncate text-xs text-muted-foreground">{from}</p>}
      </div>
      {date !== '' && <p className="mt-0.5 text-xs text-muted-foreground">{date}</p>}
    </div>
  );
}
