// SPDX-License-Identifier: GPL-3.0-only
// The outbox queue view: one account's sends that have not gone out, drawn in
// the message list pane when the folder tree's Outbox row leads. Each state
// offers exactly the actions the engine accepts (a queued or refused send can
// go now or be withdrawn, a send in flight cannot be called back, an
// unconfirmed one offers only the user's answer), and the removal verbs mark
// their row so the send-result toast never reads the user's own hand as a
// delivery, with the mark taken back when the host refuses the action and
// judged by the snapshot that follows when the engine answers.

import { dispatch, intents, type Intent, type QueuedKey, type QueuedRow } from '@/services/ipc';
import type { HeldSend } from '@/services/sending';
import { useSendingStore } from '@/stores/sendingStore';

import { useSendResultStore } from './sendResult';

/** One queued send's state word, as the queue spells it. */
const stateWord: Record<QueuedRow['state'], string> = {
  queued: 'Queued',
  sending: 'Sending',
  unconfirmed: 'Unconfirmed',
  failed: 'Failed',
};

/** The arrival instant as the queue shows it; nothing when it is unknown. */
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
function OutboxQueueRow({ row }: { row: QueuedRow }) {
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

/** One held send's state word, as the hold spells it. */
const heldWord: Record<HeldSend['state'], string> = {
  checking: 'Checking',
  needs_answer: 'Needs your answer',
  blocked: 'Held',
};

/** The one stable empty list a held-free snapshot selects (a fresh `[]` per
 * render would spin the store's equality check). */
const NO_HELD: HeldSend[] = [];

/** One send the plugin gate is holding: the reason, the plugin named, and the
 * actions the hold's state accepts. */
function HeldRow({ held }: { held: HeldSend }) {
  const actions: Array<{ label: string; run: () => void }> = [];
  if (held.state === 'needs_answer') {
    actions.push({
      label: 'Send anyway',
      run: () => void dispatch(intents.answerGate(held.hold, true)),
    });
    actions.push({
      label: 'Deny',
      run: () => void dispatch(intents.answerGate(held.hold, false)),
    });
  }
  if (held.state === 'blocked') {
    actions.push({ label: 'Retry', run: () => void dispatch(intents.retryHeld(held.hold)) });
    actions.push({ label: 'Withdraw', run: () => void dispatch(intents.withdrawHeld(held.hold)) });
  }
  return (
    <li
      className="flex flex-col gap-1 border-b border-border px-3 py-2 last:border-b-0"
      data-held={held.state}
    >
      <div className="flex items-baseline justify-between gap-2">
        <span className="min-w-0 truncate text-xs text-foreground">
          {held.subject === '' ? '(no subject)' : held.subject}
        </span>
        <span className="shrink-0 text-xs text-muted-foreground">{heldWord[held.state]}</span>
      </div>
      <div className="flex items-center justify-between gap-2">
        <span className="min-w-0 truncate text-[11px] text-muted-foreground">
          {held.state === 'checking'
            ? `Waiting on ${held.plugin}`
            : `${held.plugin}: ${held.reason}`}
        </span>
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

/** The account's queue as the list pane's body, with the gate's held sends below it. */
export function OutboxQueue({ account }: { account: string }) {
  const rows = useSendingStore((s) => s.snapshot.rows);
  const held = useSendingStore((s) => s.snapshot.held);
  const own = rows.filter((row) => row.queued.account === account);
  const heldOwn = (held ?? NO_HELD).filter((row) => row.account === account);

  if (own.length === 0 && heldOwn.length === 0) {
    return <p className="p-4 text-sm text-muted-foreground">No sends waiting</p>;
  }
  return (
    <ul aria-label="Outbox" className="min-h-0 flex-1 overflow-y-auto">
      {own.map((row) => (
        <OutboxQueueRow key={`${row.queued.account}/${row.queued.op}`} row={row} />
      ))}
      {heldOwn.map((row) => (
        <HeldRow key={row.hold} held={row} />
      ))}
    </ul>
  );
}
