// SPDX-License-Identifier: GPL-3.0-only
// The sweep dialog: one sender's mail in the current folder, previewed
// over the loaded window and then archived or trashed in one confirmed
// batch. The matching rows are the list store's own rows filtered by
// sender_address, the list filter's predicate pattern, so the count names
// the loaded window only and the batch never leaves it. The act verbs stop
// at trash and the confirm is the guard: no undo, the bulk toolbar's own
// stance. The row menu's Clean up entry opens it through the sweep store.

import { useState } from 'react';

import { useDialogFocus } from '@/components/ui/useDialogFocus';
import { dispatch, intents } from '@/services/ipc';
import { useListStore } from '@/stores/listStore';
import type { MessageRowView } from '@/services/surfaces';

import { useSweepStore } from './sweepStore';

/** The action the confirmed batch runs. */
type SweepAction = 'archive' | 'trash';

/** How many subjects the preview lists before it names the remainder. */
const PREVIEW_LIMIT = 5;

/** The reason the confirm stands disabled while no loaded row matches. */
const EMPTY_REASON = 'No loaded messages from this sender.';

/** Fires one intent per matching row, fire-and-forget: the bulk toolbar's
 *  own loop, the snapshots carrying the outcomes back. */
function forEach(rows: MessageRowView[], act: (row: MessageRowView) => void): void {
  for (const row of rows) {
    act(row);
  }
}

/** The store gate: the dialog stands while a sender is named, keyed by it
 *  so every open starts on the Archive default with its own focus move. */
export function SweepDialog() {
  const sender = useSweepStore((s) => s.sender);
  if (sender === null) {
    return null;
  }
  return <SweepDialogBody key={sender} sender={sender} />;
}

/** The modal over one sender's matching rows in the loaded window. */
function SweepDialogBody({ sender }: { sender: string }) {
  const rows = useListStore((s) => s.rows);
  const [action, setAction] = useState<SweepAction>('archive');
  const dialogRef = useDialogFocus<HTMLDivElement>();

  const matching = rows.filter((row) => row.sender_address === sender);
  const count = matching.length;
  const noun = count === 1 ? 'message' : 'messages';
  const preview = matching.slice(0, PREVIEW_LIMIT);
  const remainder = count - preview.length;
  const confirmLabel =
    action === 'archive' ? `Archive ${count} ${noun}` : `Move ${count} ${noun} to Trash`;

  const close = (): void => useSweepStore.getState().close();
  const confirm = (): void => {
    if (count === 0) {
      return;
    }
    forEach(matching, (row) =>
      void dispatch(
        action === 'archive' ? intents.archiveMessage(row.key) : intents.moveToTrash(row.key),
      ),
    );
    close();
  };

  const choice =
    'whitespace-nowrap rounded-[var(--radius-xs)] px-2 py-0.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';
  const footer =
    'rounded-[var(--radius)] px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]';

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          close();
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Clean up sender"
        ref={dialogRef}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            close();
          }
        }}
        className="flex max-h-full w-[420px] flex-col overflow-y-auto rounded-xl border border-border-subtle bg-card p-5 shadow-[var(--shadow-lg)] focus-visible:outline-none"
      >
        <h2 className="mb-3 text-sm font-semibold text-foreground">Clean up sender</h2>

        <p className="truncate text-xs text-muted-foreground" title={sender}>
          {sender}
        </p>
        <p className="mt-2 text-sm text-foreground">
          {count} {noun} in this folder
        </p>
        <p className="mt-0.5 text-[11px] text-muted-text">Counts the loaded rows only.</p>

        {count > 0 && (
          <ul className="mt-2 flex flex-col gap-0.5 border-t border-border-subtle pt-2 text-xs text-muted-foreground">
            {preview.map((row) => (
              <li key={JSON.stringify(row.key)} className="truncate">
                {row.subject === '' ? '(no subject)' : row.subject}
              </li>
            ))}
            {remainder > 0 && <li className="text-muted-text">…and {remainder} more</li>}
          </ul>
        )}

        <div
          role="group"
          aria-label="Action"
          className="mt-3 flex shrink-0 items-center gap-0.5 self-start rounded-md bg-muted p-0.5"
        >
          <button
            type="button"
            aria-pressed={action === 'archive'}
            onClick={() => setAction('archive')}
            className={`${choice} ${
              action === 'archive'
                ? 'bg-background font-medium text-foreground shadow-[var(--shadow-sm)]'
                : 'text-muted-text hover:text-foreground'
            }`}
          >
            Archive
          </button>
          <button
            type="button"
            aria-pressed={action === 'trash'}
            onClick={() => setAction('trash')}
            className={`${choice} ${
              action === 'trash'
                ? 'bg-background font-medium text-foreground shadow-[var(--shadow-sm)]'
                : 'text-muted-text hover:text-foreground'
            }`}
          >
            Move to Trash
          </button>
        </div>

        {count === 0 && <p className="mt-2 text-xs text-muted-foreground">{EMPTY_REASON}</p>}

        <div className="mt-4 flex justify-end gap-2 border-t border-border-subtle pt-3">
          <button
            type="button"
            onClick={close}
            className={`${footer} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={confirm}
            disabled={count === 0}
            title={count === 0 ? EMPTY_REASON : undefined}
            className={`${footer} bg-primary font-medium text-primary-foreground shadow-[var(--shadow-sm)] hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50`}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}