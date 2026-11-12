// SPDX-License-Identifier: GPL-3.0-only
// The quick steps store: upsert (add and edit in place), delete, the ordered
// list, the five-step cap, the device-local persistence (a malformed blob
// reads as empty, malformed entries drop), the manager gate, and the one-line
// description the ribbon tooltip and the manager list share.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  MAX_QUICK_STEPS,
  QUICK_STEP_ACTIONS,
  describeQuickStep,
  useQuickStepsStore,
  type QuickStep,
} from './quickStepsStore';
import { useFolderStore } from '@/stores/folderStore';
import type { AccountFolderRow } from '@/services/surfaces';

const STORAGE_KEY = 'astra.quickSteps';

/** One step, overridable per test. */
function step(id: string, over: Partial<QuickStep> = {}): QuickStep {
  return { id, name: `Step ${id}`, action: 'archive', ...over };
}

/** The stored ids, in order. */
function ids(): string[] {
  return useQuickStepsStore.getState().steps.map((s) => s.id);
}

/** One account group with two folders, for the description lookup. */
function accountGroup(): AccountFolderRow {
  return {
    account_id: 'work',
    display_name: 'Work',
    address: 'me@work.example',
    expanded: true,
    folders: [
      { key: 'INBOX', name: 'Inbox', role: 'inbox', unread: 0, depth: 0, has_children: false, expanded: true },
      { key: 'Receipts', name: 'Receipts', unread: 0, depth: 0, has_children: false, expanded: true },
    ],
  };
}

beforeEach(() => {
  localStorage.clear();
  useQuickStepsStore.setState({ steps: [], managerOpen: false });
  useFolderStore.setState({ accounts: [] });
});

afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
});

describe('the quick steps store', () => {
  it('catalogues the seven actions the intent surface serves', () => {
    expect(QUICK_STEP_ACTIONS.map((a) => a.id)).toEqual([
      'archive',
      'trash',
      'mark-read',
      'mark-unread',
      'flag',
      'unflag',
      'move',
    ]);
  });

  it('adds steps in order and keeps the list ordered', () => {
    const store = useQuickStepsStore.getState();
    store.upsert(step('a'));
    store.upsert(step('b'));
    store.upsert(step('c'));
    expect(ids()).toEqual(['a', 'b', 'c']);
  });

  it('edits a step in place by id, keeping its position', () => {
    const store = useQuickStepsStore.getState();
    store.upsert(step('a'));
    store.upsert(step('b'));
    store.upsert(step('a', { name: 'Renamed', action: 'trash' }));

    expect(ids()).toEqual(['a', 'b']);
    expect(useQuickStepsStore.getState().steps[0]).toEqual({
      id: 'a',
      name: 'Renamed',
      action: 'trash',
    });
  });

  it('deletes a step by id; an absent id changes nothing', () => {
    const store = useQuickStepsStore.getState();
    store.upsert(step('a'));
    store.upsert(step('b'));
    store.delete('a');
    expect(ids()).toEqual(['b']);
    store.delete('gone');
    expect(ids()).toEqual(['b']);
  });

  it('caps the list at five, dropping the overflow on add', () => {
    const store = useQuickStepsStore.getState();
    for (let i = 0; i < MAX_QUICK_STEPS + 2; i += 1) {
      store.upsert(step(`s${i}`));
    }
    expect(ids()).toEqual(['s0', 's1', 's2', 's3', 's4']);
    expect(MAX_QUICK_STEPS).toBe(5);
  });

  it('still edits an existing step while the list stands at the cap', () => {
    const store = useQuickStepsStore.getState();
    for (let i = 0; i < MAX_QUICK_STEPS; i += 1) {
      store.upsert(step(`s${i}`));
    }
    store.upsert(step('s4', { name: 'Edited at the cap' }));
    expect(ids()).toHaveLength(MAX_QUICK_STEPS);
    expect(useQuickStepsStore.getState().steps[4]?.name).toBe('Edited at the cap');
  });

  it('persists every change to localStorage', () => {
    const store = useQuickStepsStore.getState();
    store.upsert(step('a', { action: 'move', target: { account: 'work', folderKey: 'Receipts' }, markRead: true }));
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(
      useQuickStepsStore.getState().steps,
    );

    store.delete('a');
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual([]);
  });

  it('hydrates from localStorage on load', async () => {
    const stored = [step('a', { action: 'flag', markRead: true })];
    localStorage.setItem(STORAGE_KEY, JSON.stringify(stored));
    vi.resetModules();
    const fresh = await import('./quickStepsStore');
    expect(fresh.useQuickStepsStore.getState().steps).toEqual(stored);
  });

  it('reads a malformed blob as empty', async () => {
    localStorage.setItem(STORAGE_KEY, '{not json');
    vi.resetModules();
    const fresh = await import('./quickStepsStore');
    expect(fresh.useQuickStepsStore.getState().steps).toEqual([]);
  });

  it('reads a non-array blob as empty', async () => {
    localStorage.setItem(STORAGE_KEY, '{"steps":[]}');
    vi.resetModules();
    const fresh = await import('./quickStepsStore');
    expect(fresh.useQuickStepsStore.getState().steps).toEqual([]);
  });

  it('drops the malformed entries and keeps the well-formed ones', async () => {
    const good = step('good');
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify([
        good,
        null,
        42,
        { id: '', name: 'No id', action: 'archive' },
        { id: 'x', name: '  ', action: 'archive' },
        { id: 'x', name: 'Bad action', action: 'snooze' },
        { id: 'x', name: 'Bad target', action: 'move', target: { account: 'work' } },
        { id: 'x', name: 'Bad modifier', action: 'archive', markRead: 'yes' },
      ]),
    );
    vi.resetModules();
    const fresh = await import('./quickStepsStore');
    expect(fresh.useQuickStepsStore.getState().steps).toEqual([good]);
  });

  it('keeps a well-formed move target and drops a malformed one whole', async () => {
    const good = step('m', { action: 'move', target: { account: 'work', folderKey: 'Receipts' } });
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify([
        good,
        step('bad', { action: 'move', target: 'inbox' as unknown as QuickStep['target'] }),
      ]),
    );
    vi.resetModules();
    const fresh = await import('./quickStepsStore');
    expect(fresh.useQuickStepsStore.getState().steps).toEqual([good]);
  });

  it('clamps an over-long persisted list to the cap on load', async () => {
    const stored = Array.from({ length: MAX_QUICK_STEPS + 3 }, (_, i) => step(`s${i}`));
    localStorage.setItem(STORAGE_KEY, JSON.stringify(stored));
    vi.resetModules();
    const fresh = await import('./quickStepsStore');
    expect(fresh.useQuickStepsStore.getState().steps).toHaveLength(MAX_QUICK_STEPS);
  });

  it('opens and closes the manager through the gate', () => {
    expect(useQuickStepsStore.getState().managerOpen).toBe(false);
    useQuickStepsStore.getState().openManager();
    expect(useQuickStepsStore.getState().managerOpen).toBe(true);
    useQuickStepsStore.getState().closeManager();
    expect(useQuickStepsStore.getState().managerOpen).toBe(false);
  });
});

describe('the quick step description', () => {
  it('names each plain action', () => {
    expect(describeQuickStep(step('a', { action: 'archive' }))).toBe('Archive');
    expect(describeQuickStep(step('a', { action: 'trash' }))).toBe('Move to Trash');
    expect(describeQuickStep(step('a', { action: 'mark-read' }))).toBe('Mark as read');
    expect(describeQuickStep(step('a', { action: 'mark-unread' }))).toBe('Mark as unread');
    expect(describeQuickStep(step('a', { action: 'flag' }))).toBe('Flag');
    expect(describeQuickStep(step('a', { action: 'unflag' }))).toBe('Unflag');
  });

  it('names the move destination from the folder store, falling back to the key', () => {
    useFolderStore.setState({ accounts: [accountGroup()] });
    const target = { account: 'work', folderKey: 'Receipts' };
    expect(describeQuickStep(step('m', { action: 'move', target }))).toBe('Move to Receipts');

    const gone = { account: 'work', folderKey: 'Gone' };
    expect(describeQuickStep(step('m', { action: 'move', target: gone }))).toBe('Move to Gone');
  });

  it('appends the mark-read-first modifier', () => {
    expect(describeQuickStep(step('a', { action: 'archive', markRead: true }))).toBe(
      'Archive, mark read first',
    );
  });
});