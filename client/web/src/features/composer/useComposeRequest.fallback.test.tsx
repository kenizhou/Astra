// SPDX-License-Identifier: GPL-3.0-only
// The compose request flow outside Tauri: a plain browser holds no windows,
// so a landed request falls back to the inline composer, kylins' fallback.
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: false,
}));

import type { ComposeRequestSnapshot } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';

import { useComposerStore } from './composerStore';
import { useComposeRequestHydration } from './useComposeRequest';

/** A reply request for the fixture account. */
function replyRequest(): ComposeRequestSnapshot {
  return {
    kind: 'reply',
    from: 'work',
    to: 'ada@example.test',
    cc: '',
    subject: 'Re: Quarterly report',
    in_reply_to: 'orig-1@server.test',
    document: { blocks: [], attachments: [] },
  };
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work', address: 'me@work.example' }] });
  useComposeRequestStore.setState({ snapshot: null });
  useComposerStore.getState().closePane();
});

afterEach(() => {
  useComposerStore.getState().closePane();
});

describe('the compose request route outside Tauri', () => {
  it('opens the inline composer on a landed request', () => {
    renderHook(() => useComposeRequestHydration());

    act(() => {
      useComposeRequestStore.getState().applySnapshot(replyRequest());
    });

    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.to).toBe('ada@example.test');
    expect(state.subject).toBe('Re: Quarterly report');
  });
});
