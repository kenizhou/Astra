// SPDX-License-Identifier: GPL-3.0-only
// The outbox panel: every account's sends that have not gone out, over the
// shell's corner. Each state offers exactly the actions the engine accepts
// (a queued or refused send can go now or be withdrawn, a send in flight
// cannot be called back, an unconfirmed one offers only the user's answer),
// opening the panel asks the host to rebuild the queue, and the removal
// verbs mark their row so the send-result toast never reads the user's own
// hand as a delivery, with the mark taken back when the host refuses the
// action and judged by the snapshot that follows when the engine answers.
// The tool rail's toggle carries the outstanding count.

import { useEffect } from 'react';

import { OutboxIcon } from '@/components/icons';
import { RAIL_BUTTON_CLASS, railButtonColours } from '@/components/layout/railButton';
import { dispatch, intents, type Intent, type QueuedKey, type QueuedRow } from '@/services/ipc';
import { useSendingStore } from '@/stores/sendingStore';
import { useUiStore } from '@/stores/uiStore';

import { useSendResultStore } from './sendResult';

/** One queued send's state word, as the panel spells it. */
const stateWord: Record<QueuedRow['state'], string> = {
  queued: 'Queued',
  sending: 'Sending',
  unconfirmed: 'Unconfirmed',
  failed: 'Failed',
};

/** The arrival instant as the panel shows it; nothing when it is unknown. */
function arrivalWord(arrivedAt: string): string {
  return arrivedAt === '' ? '' : `${new Date(arrivedAt).toISOString().slice(11, 16)} UTC`;
}

const actionClass =
  'rounded-[var(--radius-xs)] px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

/**
 * Dispatches a removal verb with its settled mark recorded first, so the
 * toast never reads the user's own hand as a delivery. A dispatch the host
 * refused never reached the queue: the mark comes back off at once. One the
 * engine answered leaves the judging to the snapshot that follows, which
 * takes the mark back off a row still standing (the send went in flight
 * under the click), so the send's later genuine delivery still toasts.
 */
function dispatchRemoval(intent: Intent, queued: QueuedKey): void {
  const sendResult = useSendResultStore.getState();
  sendResult.noteUserSettled(queued);
  void dispatch(intent).then(
    () => sendResult.noteEngineAnswered(queued),
    () => sendResult.unsettle(queued),
  );
}

/** One queued send, with the actions its state accepts. */
function OutboxRow({ row }: { row: QueuedRow }) {
  const actions: Array<{ label: string; run: () => void }> = [];
  if (row.state === 'queued' || row.state === 'failed') {
    // A refused send's first attempt is gone, so the verb says what it does.
    actions.push({
      label: row.state === 'queued' ? 'Send now' : 'Send again',
      run: () => void dispatch(intents.sendQueuedNow(row.queued)),
    });
    actions.push({
      label: 'Cancel',
      run: () => dispatchRemoval(intents.cancelQueuedSend(row.queued), row.queued),
    });
  }
  if (row.state === 'unconfirmed') {
    actions.push({
      label: 'Mark as sent',
      run: () => dispatchRemoval(intents.confirmQueuedSent(row.queued), row.queued),
    });
    actions.push({
      label: 'Send again',
      run: () => void dispatch(intents.confirmQueuedNotSent(row.queued)),
    });
  }

  return (
    <li className="flex flex-col gap-1 border-b border-border px-3 py-2 last:border-b-0">
      <div className="flex items-baseline justify-between gap-2">
        <span className="min-w-0 truncate text-xs text-foreground">
          {row.subject === '' ? '(no subject)' : row.subject}
        </span>
        <span className="shrink-0 text-xs text-muted-foreground">{stateWord[row.state]}</span>
      </div>
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] text-muted-foreground">{arrivalWord(row.arrived_at)}</span>
        <div className="flex items-center gap-1">
          {actions.map((action) => (
            <button key={action.label} type="button" onClick={action.run} className={actionClass}>
              {action.label}
            </button>
          ))}
        </div>
      </div>
    </li>
  );
}

/** The outbox panel over the shell's corner; renders nothing when shut. */
export function OutboxPanel() {
  const open = useUiStore((s) => s.outboxOpen);
  const rows = useSendingStore((s) => s.snapshot.rows);

  useEffect(() => {
    if (!open) {
      return;
    }
    // A show that finds nothing moved says nothing; the pull answers the rest.
    void dispatch(intents.showOutbox());
  }, [open]);

  if (!open) {
    return null;
  }

  return (
    <section
      aria-label="Outbox"
      className="absolute bottom-[calc(var(--status-h)+12px)] right-3 z-[var(--z-popover)] flex max-h-[60vh] w-[340px] flex-col overflow-hidden rounded-[var(--radius)] border border-border bg-surface shadow-lg"
    >
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between border-b border-border px-3">
        <span className="text-xs font-medium text-foreground">Outbox</span>
        <button
          type="button"
          aria-label="Close outbox"
          onClick={() => useUiStore.getState().setOutboxOpen(false)}
          className={actionClass}
        >
          Close
        </button>
      </div>
      {rows.length === 0 ? (
        <p className="px-3 py-3 text-xs text-muted-foreground">No sends waiting</p>
      ) : (
        <ul className="min-h-0 flex-1 overflow-y-auto">
          {rows.map((row) => (
            <OutboxRow
              key={`${row.queued.account}/${row.queued.op}`}
              row={row}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

/** The tool rail's outbox toggle, carrying the outstanding send count. */
export function OutboxStatus() {
  const open = useUiStore((s) => s.outboxOpen);
  const count = useSendingStore((s) => s.snapshot.rows.length);

  return (
    <button
      type="button"
      aria-label={count > 0 ? `Outbox ${count}` : 'Outbox'}
      aria-pressed={open}
      title={count > 0 ? `Outbox, ${count} waiting` : 'Outbox'}
      onClick={() => useUiStore.getState().setOutboxOpen(!open)}
      className={`${RAIL_BUTTON_CLASS} ${railButtonColours(open)}`}
    >
      <OutboxIcon size={22} />
      {count > 0 && (
        <span
          aria-hidden="true"
          className="absolute right-1 top-1 grid h-4 min-w-4 place-items-center rounded-full bg-primary px-1 text-[10px] font-medium text-primary-foreground"
        >
          {count}
        </span>
      )}
    </button>
  );
}
