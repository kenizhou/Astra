// SPDX-License-Identifier: GPL-3.0-only
// The draft-status store: the snapshot a pull lands, and the null that clears
// it when the composition is forgotten.
import { beforeEach, describe, expect, it } from 'vitest';

import { useDraftStore } from './draftStore';
import type { DraftStatusSnapshot } from '@/services/ipc';

/** A saved hint for `composition`, overridable per test. */
function snapshot(
  composition: string,
  over: Partial<DraftStatusSnapshot> = {},
): DraftStatusSnapshot {
  return {
    composition,
    state: 'saved',
    saved_at: '2026-10-09T08:30:00Z',
    ...over,
  };
}

beforeEach(() => {
  useDraftStore.setState({ snapshot: null });
});

describe('the draft store', () => {
  it('holds the applied snapshot and replaces it wholesale', () => {
    useDraftStore.getState().applySnapshot(snapshot('compose-1'));
    expect(useDraftStore.getState().snapshot?.composition).toBe('compose-1');

    useDraftStore.getState().applySnapshot(snapshot('compose-2', { state: 'saving', saved_at: '' }));

    expect(useDraftStore.getState().snapshot).toEqual({
      composition: 'compose-2',
      state: 'saving',
      saved_at: '',
    });
  });

  it('clears on null', () => {
    useDraftStore.getState().applySnapshot(snapshot('compose-1'));

    useDraftStore.getState().applySnapshot(null);

    expect(useDraftStore.getState().snapshot).toBeNull();
  });
});
