// SPDX-License-Identifier: GPL-3.0-only
// The New message and mailto routes outside Tauri: a plain browser dev
// server holds no windows, so each falls back to the inline composer,
// kylins' fallback, the mailto one opening prefilled over the link's
// account.
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: false,
}));

import { useAccountStore } from '@/stores/accountStore';

import { useComposerStore } from './composerStore';
import { openNewMessageComposer } from './composeWindow';
import { openMailtoComposer } from './mailtoOpen';

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

describe('the mailto route outside Tauri', () => {
  it('opens the inline composer prefilled', () => {
    openMailtoComposer({
      from: 'work',
      to: 'ada@example.test',
      cc: 'tim@example.test',
      bcc: '',
      subject: 'Lunch',
      body: 'Noon?',
    });

    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.from).toBe('work');
    expect(state.fromLabel).toBe('Work');
    expect(state.to).toBe('ada@example.test');
    expect(state.cc).toBe('tim@example.test');
    expect(state.subject).toBe('Lunch');
    expect(state.request).toBeNull();
  });
});
