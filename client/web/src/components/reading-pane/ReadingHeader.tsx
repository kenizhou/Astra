// SPDX-License-Identifier: GPL-3.0-only
// The reading header: the subject at the subject type scale, then the sender
// block with the avatar beside a semibold name over the bare address, and the
// full UTC stamp right-aligned on that row. The avatar resolves through the
// people command on the sender's address, the one resolution every surface
// shares. The stamp derives from the UTC parts the same way the list's does,
// with the year always shown and the time of day appended in the form the
// clock setting names.

import { Avatar } from '@/features/people/Avatar';
import { useSettingsStore } from '@/features/settings/settingsStore';

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

/**
 * Formats the open message's sent_at as `9 Oct 2026, 08:30`, or the 12-hour
 * form with its meridiem when the clock setting says so; empty when unknown.
 */
export function formatReadingDate(sentAt: string, twentyFour = true): string {
  if (sentAt === '') {
    return '';
  }
  const stamp = new Date(sentAt);
  if (Number.isNaN(stamp.getTime())) {
    return '';
  }
  const two = (value: number): string => String(value).padStart(2, '0');
  const month = MONTHS[stamp.getUTCMonth()] ?? '';
  if (twentyFour) {
    return `${stamp.getUTCDate()} ${month} ${stamp.getUTCFullYear()}, ${two(stamp.getUTCHours())}:${two(stamp.getUTCMinutes())}`;
  }
  const hour = stamp.getUTCHours();
  const meridiem = hour < 12 ? 'AM' : 'PM';
  const twelve = hour % 12 === 0 ? 12 : hour % 12;
  return `${stamp.getUTCDate()} ${month} ${stamp.getUTCFullYear()}, ${twelve}:${two(stamp.getUTCMinutes())} ${meridiem}`;
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
  const twentyFour = useSettingsStore((s) => s.settings.twenty_four_clock);
  const hasSender = fromName !== '' || fromAddress !== '';
  const displayName = fromName === '' ? fromAddress : fromName;
  const date = formatReadingDate(sentAt, twentyFour);
  return (
    <div className="shrink-0 border-b border-border-subtle px-4 pb-3 pt-4">
      <h2 className="truncate [font:var(--text-subject)] text-foreground">
        {subject === '' ? '(no subject)' : subject}
      </h2>
      {hasSender ? (
        <div className="mt-2.5 flex items-center gap-3">
          {fromAddress !== '' && <Avatar address={fromAddress} size={36} />}
          <div className="min-w-0 flex-1">
            <p className="truncate text-[13px] font-semibold text-foreground">{displayName}</p>
            {fromName !== '' && fromAddress !== '' && (
              <p className="truncate text-xs text-muted-text">{fromAddress}</p>
            )}
          </div>
          {date !== '' && (
            <span className="shrink-0 self-start text-xs tabular-nums text-muted-text">
              {date}
            </span>
          )}
        </div>
      ) : (
        date !== '' && <p className="mt-1 text-xs tabular-nums text-muted-text">{date}</p>
      )}
    </div>
  );
}
