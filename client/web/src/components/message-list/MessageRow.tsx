// SPDX-License-Identifier: GPL-3.0-only
// One message row: the sender monogram, the sender and subject in bold
// while unread, the UTC-derived stamp, and the state icons on the right.
// A 2px state ribbon on the row's left edge marks unread mail in primary;
// the row's own actions (mark read, flag) ride the trailing edge and show
// while the row is hovered or selected, with a flagged row keeping its
// flag visible so the state never hides.

import { useState } from 'react';

import { FlagIcon, PaperclipIcon } from '@/components/icons';
import { dispatch, intents } from '@/services/ipc';
import { useListStore } from '@/stores/listStore';
import type { RowKey, MessageRowView } from '@/services/surfaces';
import { Monogram } from './Monogram';
import { DENSITY_ROW_CLASS } from './density';
import { formatSentAt } from './sentAt';
import { useSettingsStore } from '@/features/settings/settingsStore';

interface MessageRowProps {
  /** The row's snapshot data; the single source of everything drawn. */
  row: MessageRowView;
  /** Whether the local selection names this row. */
  selected: boolean;
  /** The clock the stamp is formatted against. */
  now: Date;
  /** Records the local selection when the row is clicked. */
  onSelect: (key: RowKey) => void;
}

export function MessageRow({ row, selected, now, onSelect }: MessageRowProps) {
  const [hovered, setHovered] = useState(false);
  const density = useListStore((s) => s.density);
  const actionsVisible = hovered || selected;
  const bold = row.unread ? 'font-semibold' : 'font-normal';
  const sender = row.sender_name || row.sender_address;
  const count = row.thread_count;
  const countLabel =
    count === undefined ? undefined : `${count} message${count === 1 ? '' : 's'}`;

  const markRead = (event: React.MouseEvent) => {
    event.stopPropagation();
    // The intent names the state being set, and the button flips what the
    // row is: an unread row is being marked read, a read one unread. The
    // unread flag the row carries is exactly that boolean.
    void dispatch(intents.markRead(row.key, row.unread));
  };
  const toggleFlag = (event: React.MouseEvent) => {
    event.stopPropagation();
    void dispatch(intents.setFlagged(row.key, !row.flagged));
  };

  return (
    <div
      role="option"
      aria-selected={selected}
      onClick={() => onSelect(row.key)}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      className={`relative flex w-full cursor-pointer items-start gap-2.5 border-b border-border-subtle px-3 ${DENSITY_ROW_CLASS[density]} ${
        selected ? 'bg-selected' : 'hover:bg-hover'
      }`}
    >
      {/* The read state reads off the left edge: a 2px ribbon in primary
          while unread, transparent once read. */}
      <span
        aria-hidden="true"
        className={`absolute bottom-0 left-0 top-0 w-[2px] ${row.unread ? 'bg-primary' : 'bg-transparent'}`}
      />
      <Monogram address={row.sender_address} />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline justify-between gap-2">
          <span className={`min-w-0 truncate text-xs text-foreground ${bold}`}>{sender}</span>
          <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground">
            {formatSentAt(row.sent_at, now, useSettingsStore.getState().settings.twenty_four_clock)}
          </span>
        </div>
        <div className="flex items-center justify-between gap-2">
          <span className={`min-w-0 truncate text-[13px] text-foreground ${bold}`}>
            {row.subject === '' ? '(no subject)' : row.subject}
          </span>
          <span className="flex shrink-0 items-center gap-1 text-muted-foreground">
            {count !== undefined && (
              <span
                title={countLabel}
                aria-label={countLabel}
                className="rounded-full bg-muted px-1.5 py-0.5 text-[10px] leading-none text-muted-foreground"
              >
                {count}
              </span>
            )}
            {row.has_attachments && (
              <span title="Has attachments" aria-label="Has attachments" className="inline-flex">
                <PaperclipIcon size={14} />
              </span>
            )}
            {(actionsVisible || row.flagged) && (
              <button
                type="button"
                aria-label={row.flagged ? 'Unflag' : 'Flag'}
                aria-pressed={row.flagged}
                onClick={toggleFlag}
                className={`inline-flex rounded-[var(--radius-xs)] p-0.5 hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                  row.flagged ? 'text-warning' : 'hover:text-foreground'
                }`}
              >
                <FlagIcon size={14} />
              </button>
            )}
            {actionsVisible && (
              <button
                type="button"
                aria-label={row.unread ? 'Mark as read' : 'Mark as unread'}
                onClick={markRead}
                className="rounded-[var(--radius-xs)] px-1 py-0.5 text-[10px] hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                {row.unread ? 'Read' : 'Unread'}
              </button>
            )}
          </span>
        </div>
      </div>
    </div>
  );
}
