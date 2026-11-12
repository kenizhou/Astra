// SPDX-License-Identifier: GPL-3.0-only
// The quick step runner: each action dispatches the exact intent payload over
// the rows, the mark-read modifier runs FIRST over the eligible rows, move
// batches the message refs and skips conversation rows and foreign-account
// rows with a counted toast, and the selection resolves from the list store
// (the multi-selection, the single-anchor fallback, nothing selected runs
// nothing).
import { beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { quickStepTargets, runQuickStep, runQuickStepOverSelection } from './runQuickStep';
import type { QuickStep } from './quickStepsStore';
import { useSwipeToastStore } from '@/components/message-list/swipeToastStore';
import { useListStore } from '@/stores/listStore';
import type { MessageRowView, RowKey } from '@/services/surfaces';

/** One flat message row, overridable per test. */
function row(key: string, over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key: { message: { account: 'work', key } },
    account_id: 'work',
    sender_name: 'Ada Lovelace',
    sender_address: 'ada@example.test',
    subject: `Subject ${key}`,
    sent_at: '2026-11-12T08:30:00Z',
    unread: true,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

const a1 = row('a1');
const a2 = row('a2', { unread: false, flagged: true });
const thread = row('t1', { key: { thread: { account: 'work', thread_id: 'th-1' } } as RowKey });
const foreign = row('f1', {
  key: { message: { account: 'home', key: 'f1' } } as RowKey,
  account_id: 'home',
});

/** One step, overridable per test. */
function step(over: Partial<QuickStep> = {}): QuickStep {
  return { id: 'qs1', name: 'File it', action: 'archive', ...over };
}

/** The intents dispatched so far, in order. */
function dispatched(): unknown[] {
  return ipc.dispatch.mock.calls.map((call) => call[0]);
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useSwipeToastStore.setState({ note: null, undo: null });
  useListStore.setState({ rows: [], selectedKeys: [], selectedKey: null, anchorKey: null, search: null });
});

describe('the action-to-intent mapping', () => {
  it('archives every row with the exact payload', () => {
    runQuickStep(step({ action: 'archive' }), [a1, a2]);
    expect(dispatched()).toEqual([
      { archive_message: { row: a1.key } },
      { archive_message: { row: a2.key } },
    ]);
  });

  it('trashes every row with the exact payload', () => {
    runQuickStep(step({ action: 'trash' }), [a1, a2]);
    expect(dispatched()).toEqual([
      { move_to_trash: { row: a1.key } },
      { move_to_trash: { row: a2.key } },
    ]);
  });

  it('marks read and unread with the state the action names, not the row flip', () => {
    runQuickStep(step({ action: 'mark-read' }), [a1, a2]);
    expect(dispatched()).toEqual([
      { mark_read: { row: a1.key, read: true } },
      { mark_read: { row: a2.key, read: true } },
    ]);

    ipc.dispatch.mockClear();
    runQuickStep(step({ action: 'mark-unread' }), [a1, a2]);
    expect(dispatched()).toEqual([
      { mark_read: { row: a1.key, read: false } },
      { mark_read: { row: a2.key, read: false } },
    ]);
  });

  it('flags and unflags with the state the action names', () => {
    runQuickStep(step({ action: 'flag' }), [a1, a2]);
    expect(dispatched()).toEqual([
      { set_flagged: { row: a1.key, flagged: true } },
      { set_flagged: { row: a2.key, flagged: true } },
    ]);

    ipc.dispatch.mockClear();
    runQuickStep(step({ action: 'unflag' }), [a1, a2]);
    expect(dispatched()).toEqual([
      { set_flagged: { row: a1.key, flagged: false } },
      { set_flagged: { row: a2.key, flagged: false } },
    ]);
  });

  it('moves the message refs in one batch to the concrete folder', () => {
    runQuickStep(
      step({ action: 'move', target: { account: 'work', folderKey: 'Receipts' } }),
      [a1, a2],
    );
    expect(dispatched()).toEqual([
      {
        move_messages: {
          rows: [
            { account: 'work', key: 'a1' },
            { account: 'work', key: 'a2' },
          ],
          folder: { account: 'work', key: 'Receipts' },
        },
      },
    ]);
  });

  it('runs nothing over an empty row set', () => {
    runQuickStep(step(), []);
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(useSwipeToastStore.getState().note).toBeNull();
  });
});

describe('the mark-read-first modifier', () => {
  it('marks every row read BEFORE the filing verb dispatches', () => {
    runQuickStep(step({ action: 'archive', markRead: true }), [a1, a2]);
    expect(dispatched()).toEqual([
      { mark_read: { row: a1.key, read: true } },
      { mark_read: { row: a2.key, read: true } },
      { archive_message: { row: a1.key } },
      { archive_message: { row: a2.key } },
    ]);
  });

  it('marks read before a move, over the eligible rows only', () => {
    runQuickStep(
      step({ action: 'move', target: { account: 'work', folderKey: 'Receipts' }, markRead: true }),
      [a1, foreign, thread],
    );
    expect(dispatched()).toEqual([
      { mark_read: { row: a1.key, read: true } },
      {
        move_messages: {
          rows: [{ account: 'work', key: 'a1' }],
          folder: { account: 'work', key: 'Receipts' },
        },
      },
    ]);
  });

  it('leaves the rows alone when the modifier is absent', () => {
    runQuickStep(step({ action: 'trash' }), [a1]);
    expect(dispatched()).toEqual([{ move_to_trash: { row: a1.key } }]);
  });
});

describe('the move skips', () => {
  const target = { account: 'work', folderKey: 'Receipts' };

  it('skips conversation rows with a counted toast and moves the rest', () => {
    runQuickStep(step({ action: 'move', target }), [a1, thread, a2]);
    expect(dispatched()).toEqual([
      {
        move_messages: {
          rows: [
            { account: 'work', key: 'a1' },
            { account: 'work', key: 'a2' },
          ],
          folder: { account: 'work', key: 'Receipts' },
        },
      },
    ]);
    expect(useSwipeToastStore.getState().note).toBe(
      'Skipped 1 row: conversations cannot be moved',
    );
    expect(useSwipeToastStore.getState().undo).toBeNull();
  });

  it('skips foreign-account rows with a counted toast', () => {
    runQuickStep(step({ action: 'move', target }), [a1, foreign]);
    expect(dispatched()).toEqual([
      {
        move_messages: {
          rows: [{ account: 'work', key: 'a1' }],
          folder: { account: 'work', key: 'Receipts' },
        },
      },
    ]);
    expect(useSwipeToastStore.getState().note).toBe(
      'Skipped 1 row: the folder belongs to another account',
    );
  });

  it('counts both skip causes together', () => {
    runQuickStep(step({ action: 'move', target }), [thread, foreign, a1]);
    expect(useSwipeToastStore.getState().note).toBe(
      'Skipped 2 rows: conversations cannot be moved, the folder belongs to another account',
    );
  });

  it('dispatches no move and names every row when none is eligible', () => {
    runQuickStep(step({ action: 'move', target }), [thread, foreign]);
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(useSwipeToastStore.getState().note).toBe(
      'Skipped 2 rows: conversations cannot be moved, the folder belongs to another account',
    );
  });

  it('skips loudly when a move step carries no target', () => {
    runQuickStep(step({ action: 'move' }), [a1, a2]);
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(useSwipeToastStore.getState().note).toBe('Skipped 2 rows: the step has no target folder');
  });

  it('raises no toast when every row is eligible', () => {
    runQuickStep(step({ action: 'move', target }), [a1, a2]);
    expect(useSwipeToastStore.getState().note).toBeNull();
  });
});

describe('the selection resolution', () => {
  it('resolves the multi-selection against the standing rows', () => {
    useListStore.setState({ rows: [a1, a2, foreign], selectedKeys: [a1.key, a2.key] });
    expect(quickStepTargets()).toEqual([a1, a2]);

    runQuickStepOverSelection(step({ action: 'trash' }));
    expect(dispatched()).toEqual([
      { move_to_trash: { row: a1.key } },
      { move_to_trash: { row: a2.key } },
    ]);
  });

  it('falls back to the single anchor when no multi-selection stands', () => {
    useListStore.setState({ rows: [a1, a2], selectedKeys: [], selectedKey: a2.key });
    expect(quickStepTargets()).toEqual([a2]);

    runQuickStepOverSelection(step({ action: 'archive' }));
    expect(dispatched()).toEqual([{ archive_message: { row: a2.key } }]);
  });

  it('runs nothing with no selection at all', () => {
    useListStore.setState({ rows: [a1, a2] });
    expect(quickStepTargets()).toEqual([]);
    runQuickStepOverSelection(step());
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('names nothing for a key whose row has gone', () => {
    useListStore.setState({ rows: [a1], selectedKeys: [a2.key] });
    expect(quickStepTargets()).toEqual([]);
    runQuickStepOverSelection(step());
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('resolves against the search rows while a search stands', () => {
    useListStore.setState({
      rows: [a1],
      selectedKeys: [a2.key],
      search: { query: 'report', scope: 'current_folder', rows: [a2], depth_months: 1 },
    });
    expect(quickStepTargets()).toEqual([a2]);
  });
});