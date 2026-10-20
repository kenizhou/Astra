// SPDX-License-Identifier: GPL-3.0-only
// One message row: the sender and the UTC-derived stamp on the first line,
// the subject with its state icons on the second, the A3 hairline under
// every row. The read state is carried by the weight stops alone (560
// unread, 450 read; the A3 type voice), so the column needs no ribbons and
// no monograms. The row's own actions (mark read, flag) ride the trailing
// edge and show while the row is hovered or selected, with a flagged row
// keeping its flag visible so the state never hides. The whole row drags:
// the drag carries the selection it rides in (itself alone otherwise), keys
// only, never content.

import { useState } from 'react';

import { FlagIcon, PaperclipIcon } from '@/components/icons';
import { dispatch, intents } from '@/services/ipc';
import { useListStore } from '@/stores/listStore';
import type { RowKey, MessageRowView } from '@/services/surfaces';
import { resolveTargets } from './listSelection';
import { DENSITY_ROW_CLASS } from './density';
import { formatSentAt } from './sentAt';
import { useSettingsStore } from '@/features/settings/settingsStore';

/** The dataTransfer type a message drag carries: the selection's row keys. */
export const ROWS_DRAG_TYPE = 'application/x-astra-rows';

interface MessageRowProps {
  /** The row's snapshot data; the single source of everything drawn. */
  row: MessageRowView;
  /** Whether the local selection names this row. */
  selected: boolean;
  /** Whether the multi-selection names this row. */
  multiSelected?: boolean;
  /** Whether the row sits in the drafts folder: it wears the Draft chip. */
  draft?: boolean;
  /** The clock the stamp is formatted against. */
  now: Date;
  /** Handles the row's click, modifiers and all. */
  onSelect: (key: RowKey, event: React.MouseEvent) => void;
  /** Opens the row's context menu at the cursor. */
  onContextMenu?: (key: RowKey, event: React.MouseEvent) => void;
}

export function MessageRow({ row, selected, multiSelected = false, draft = false, now, onSelect, onContextMenu }: MessageRowProps) {
  const [hovered, setHovered] = useState(false);
  const density = useListStore((s) => s.density);
  const actionsVisible = hovered || selected;
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

  // The drag carries the selection when the row rides it, itself otherwise:
  // the Outlook semantics the selection model owns, keys only.
  const onDragStart = (event: React.DragEvent): void => {
    const store = useListStore.getState();
    const keys = resolveTargets(store.selectedKeys, row.key);
    event.dataTransfer.setData(ROWS_DRAG_TYPE, JSON.stringify(keys));
    event.dataTransfer.effectAllowed = 'move';
  };

  return (
    <div
      role="option"
      aria-selected={selected}
      draggable
      onDragStart={onDragStart}
      onClick={(event) => onSelect(row.key, event)}
      onContextMenu={(event) => {
        if (onContextMenu !== undefined) {
          event.preventDefault();
          onContextMenu(row.key, event);
        }
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      className={`relative flex w-full cursor-pointer items-start gap-2.5 border-b border-[var(--hairline)] px-3 ${DENSITY_ROW_CLASS[density]} ${
        selected ? 'bg-selected' : multiSelected ? 'bg-selected/60' : 'hover:bg-hover'
      }`}
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline justify-between gap-2">
          <span
            className={`min-w-0 truncate text-[13px] ${
              row.unread ? 'font-[560] text-foreground' : 'font-[450] text-muted-text'
            }`}
          >
            {sender}
          </span>
          <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
            {formatSentAt(row.sent_at, now, useSettingsStore.getState().settings.twenty_four_clock)}
          </span>
        </div>
        <div className="flex items-center justify-between gap-2">
          <span className="flex min-w-0 items-center gap-1.5">
            {draft && (
              <span className="shrink-0 rounded-[var(--radius-xs)] bg-destructive/10 px-1 text-[10px] font-medium text-destructive">
                Draft
              </span>
            )}
            <span
              className={`min-w-0 truncate text-[13px] ${
                row.unread ? 'font-[560] text-foreground' : 'font-[450] text-muted-text'
              }`}
            >
              {row.subject === '' ? '(no subject)' : row.subject}
            </span>
          </span>
          <span className="flex shrink-0 items-center gap-1 text-muted-foreground">
            {count !== undefined && (
              <span
                title={countLabel}
                aria-label={countLabel}
                className="rounded-full bg-muted px-1.5 py-0.5 text-[11px] leading-none text-muted-foreground"
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
