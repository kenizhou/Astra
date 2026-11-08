// SPDX-License-Identifier: GPL-3.0-only
// The share route outside Tauri: a plain browser dev server holds no
// windows, so a landing opens the inline composer with the shared files
// pending, the New message entry's own fallback.
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: false,
}));

import { useAccountStore } from '@/stores/accountStore';

import { useComposerStore } from './composerStore';
import { openShareComposer } from './shareOpen';

beforeEach(() => {
  useAccountStore.setState({
    accounts: [{ accountId: 'work', displayName: 'Work', address: 'me@work.example' }],
  });
  useComposerStore.getState().closePane();
});

describe('the share route outside Tauri', () => {
  it('opens the inline composer with the files pending', () => {
    openShareComposer([{ name: 'notes.txt', path: 'C:\\Users\\ada\\notes.txt' }]);

    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.from).toBe('work');
    expect(state.pendingShares).toEqual([
      { name: 'notes.txt', path: 'C:\\Users\\ada\\notes.txt' },
    ]);
  });
});
