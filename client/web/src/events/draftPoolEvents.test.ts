// SPDX-License-Identifier: GPL-3.0-only
// The draft-pool event mount: the tested service subscription folded into
// the ai chips store, with the dispose race a StrictMode double mount runs
// (a registration that resolves after its effect unmounted unlistens itself
// instead of leaking a live duplicate).
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: the mount subscribes only inside Tauri, so the jsdom
  // test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const ai = vi.hoisted(() => ({
  onDraftPoolLanded: vi.fn(),
  chipsFor: vi.fn(),
  dismissChips: vi.fn(),
}));

vi.mock('@/services/ai', () => ai);

import { mountDraftPoolEvents } from './draftPoolEvents';
import { useAiChipsStore } from '@/stores/aiChipsStore';
import type { MessageRef } from '@/services/ipc';

const work: MessageRef = { account: 'work', key: 'imap:v1:u1' };

/** Captures the handler the mount registered with the service. */
function registeredHandler(): (message: MessageRef) => void {
  expect(ai.onDraftPoolLanded).toHaveBeenCalled();
  return ai.onDraftPoolLanded.mock.calls[0]?.[0] as (message: MessageRef) => void;
}

beforeEach(() => {
  ai.onDraftPoolLanded.mockReset();
  ai.chipsFor.mockReset();
  ai.chipsFor.mockResolvedValue([]);
  ai.dismissChips.mockReset();
  useAiChipsStore.setState({ message: null, chips: [] });
});

describe('the draft-pool event mount', () => {
  it('subscribes through the service and folds a landing into the store', async () => {
    ai.onDraftPoolLanded.mockResolvedValue(() => undefined);
    useAiChipsStore.getState().open(work);
    await Promise.resolve();
    await Promise.resolve();
    ai.chipsFor.mockClear();

    const dispose = mountDraftPoolEvents();
    await Promise.resolve();
    registeredHandler()(work);
    await Promise.resolve();
    await Promise.resolve();

    expect(ai.chipsFor).toHaveBeenCalledWith(work);

    dispose();
  });

  it('disposes its subscription on unmount', async () => {
    const unlisten = vi.fn();
    ai.onDraftPoolLanded.mockResolvedValue(unlisten);

    const dispose = mountDraftPoolEvents();
    await Promise.resolve();
    dispose();

    expect(unlisten).toHaveBeenCalledOnce();
  });

  it('unlistens a registration that lands after its effect unmounted', async () => {
    const unlisten = vi.fn();
    let resolveRegistration: (unlisten: () => void) => void = () => undefined;
    ai.onDraftPoolLanded.mockImplementation(
      () =>
        new Promise<(unlisten: () => void) => void>((resolve) => {
          resolveRegistration = resolve;
        }),
    );

    const dispose = mountDraftPoolEvents();
    dispose();
    resolveRegistration(unlisten);
    await Promise.resolve();

    expect(unlisten).toHaveBeenCalledOnce();
  });
});
