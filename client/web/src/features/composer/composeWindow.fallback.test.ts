// SPDX-License-Identifier: GPL-3.0-only
// The New message route outside Tauri: a plain browser dev server holds no
// windows, so the route falls back to the inline composer, kylins' fallback.
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: false,
}));

import { useAccountStore } from '@/stores/accountStore';

import { useComposerStore } from './composerStore';
import { openNewMessageComposer } from './composeWindow';

beforeEach(() => {
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work', address: 'me@work.example' }] });
  useComposerStore.getState().closePane();
});

describe('the New message route outside Tauri', () => {
  it('opens the inline composer instead', () => {
    openNewMessageComposer();

    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.from).toBe('work');
    expect(state.fromLabel).toBe('Work');
  });
});
