// SPDX-License-Identifier: GPL-3.0-only
// The quick step executor: one action mapped to the intent the surface
// already serves (the swipe table's idiom), looped over the rows the way the
// bulk toolbar loops them. The mark-read modifier runs FIRST, because a
// filed row leaves the snapshot and the mark would miss it. A move batches
// the message refs into one concrete folder: a conversation cannot be named
// by a move batch and a folder belongs to one account, so those rows are
// skipped with a counted toast. Fire-and-forget throughout; the snapshots
// carry the outcomes back.

import { sameRowKey } from '@/components/message-list/rowIdentity';
import { useSwipeToastStore } from '@/components/message-list/swipeToastStore';
import { dispatch, intents, type Intent, type MessageRef } from '@/services/ipc';
import type { MessageRowView } from '@/services/surfaces';
import { useListStore } from '@/stores/listStore';

import type { QuickStep, QuickStepAction } from './quickStepsStore';

/** The actions that dispatch one intent per row; move rides its own batch. */
type RowAction = Exclude<QuickStepAction, 'move'>;

/** The intent one action dispatches over one row: the state verbs name the
 *  state the action leaves, not the flip of what the row carries. */
function rowIntent(action: RowAction, row: MessageRowView): Intent {
  switch (action) {
    case 'archive':
      return intents.archiveMessage(row.key);
    case 'trash':
      return intents.moveToTrash(row.key);
    case 'mark-read':
      return intents.markRead(row.key, true);
    case 'mark-unread':
      return intents.markRead(row.key, false);
    case 'flag':
      return intents.setFlagged(row.key, true);
    case 'unflag':
      return intents.setFlagged(row.key, false);
  }
}

/** The counted skip notice, the swipe toast's own channel with no undo. */
function skipToast(count: number, reason: string): void {
  const noun = count === 1 ? 'row' : 'rows';
  useSwipeToastStore.getState().show(`Skipped ${count} ${noun}: ${reason}`, null);
}

/** Marks the eligible rows read before the action runs: once a row is filed
 *  it leaves the snapshot, so the mark must land while the row still stands.
 *  Skipped rows stay untouched. */
function markReadFirst(step: QuickStep, rows: MessageRowView[]): void {
  if (step.markRead !== true) {
    return;
  }
  for (const row of rows) {
    void dispatch(intents.markRead(row.key, true));
  }
}

/** Runs one quick step over the given rows; an empty set runs nothing. */
export function runQuickStep(step: QuickStep, rows: MessageRowView[]): void {
  if (rows.length === 0) {
    return;
  }
  if (step.action === 'move') {
    runMove(step, rows);
    return;
  }
  markReadFirst(step, rows);
  for (const row of rows) {
    void dispatch(rowIntent(step.action, row));
  }
}

/** The move batch: only message rows of the target's own account can land
 *  in a concrete folder; the rest are skipped and counted. */
function runMove(step: QuickStep, rows: MessageRowView[]): void {
  const target = step.target;
  if (target === undefined) {
    skipToast(rows.length, 'the step has no target folder');
    return;
  }
  const eligible = rows.filter(
    (row) => 'message' in row.key && row.account_id === target.account,
  );
  const threads = rows.filter((row) => 'thread' in row.key).length;
  const foreign = rows.length - threads - eligible.length;

  markReadFirst(step, eligible);
  if (eligible.length > 0) {
    const refs: MessageRef[] = eligible.flatMap((row) =>
      'message' in row.key ? [row.key.message] : [],
    );
    void dispatch(intents.moveMessages(refs, { account: target.account, key: target.folderKey }));
  }

  const skipped = threads + foreign;
  if (skipped > 0) {
    const reasons: string[] = [];
    if (threads > 0) {
      reasons.push('conversations cannot be moved');
    }
    if (foreign > 0) {
      reasons.push('the folder belongs to another account');
    }
    skipToast(skipped, reasons.join(', '));
  }
}

/** The rows a quick step acts on: the multi-selection resolved against the
 *  standing rows, the single anchor the ribbon uses when no multi-selection
 *  stands, and nothing when neither does. A key whose row has gone names
 *  nothing; a live search's rows stand in for the folder's. */
export function quickStepTargets(): MessageRowView[] {
  const { rows, search, selectedKeys, selectedKey } = useListStore.getState();
  const keys =
    selectedKeys.length > 0 ? selectedKeys : selectedKey !== null ? [selectedKey] : [];
  if (keys.length === 0) {
    return [];
  }
  const pool = search?.rows ?? rows;
  return pool.filter((row) => keys.some((key) => sameRowKey(key, row.key)));
}

/** Runs one quick step over the list's current selection. */
export function runQuickStepOverSelection(step: QuickStep): void {
  runQuickStep(step, quickStepTargets());
}