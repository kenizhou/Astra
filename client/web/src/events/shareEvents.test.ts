// SPDX-License-Identifier: GPL-3.0-only
// The share event mount: the share channel's subscription routed into the
// composer's share open path, with the dispose race a StrictMode double
// mount runs, and the forward note's own pin: the path never logs, here
// or anywhere the event lands.
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: the mount subscribes only inside Tauri, so the jsdom
  // test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const flow = vi.hoisted(() => ({
  onShareOpen: vi.fn(),
  openShareComposer: vi.fn(),
}));

vi.mock('@/features/composer/shareOpen', () => flow);

import { mountShareEvents } from './shareEvents';
import type { SharedFile } from '@/features/composer/shareOpen';

const files: SharedFile[] = [{ name: 'notes.txt', path: 'C:\\Users\\ada\\notes.txt' }];

/** Captures the handler the mount registered with the flow. */
function registeredHandler(): (files: SharedFile[]) => void {
  expect(flow.onShareOpen).toHaveBeenCalled();
  return flow.onShareOpen.mock.calls[0]?.[0] as (files: SharedFile[]) => void;
}

beforeEach(() => {
  flow.onShareOpen.mockReset();
  flow.openShareComposer.mockReset();
});

describe('the share event mount', () => {
  it('subscribes through the flow and routes a landing into the composer', async () => {
    flow.onShareOpen.mockResolvedValue(() => undefined);
    const log = vi.spyOn(console, 'log').mockImplementation(() => undefined);

    const dispose = mountShareEvents();
    await Promise.resolve();
    registeredHandler()(files);

    expect(flow.openShareComposer).toHaveBeenCalledWith(files);
    // The forward note, load bearing: the path never logs.
    expect(log).not.toHaveBeenCalled();

    log.mockRestore();
    dispose();
  });

  it('disposes its subscription on unmount', async () => {
    const unlisten = vi.fn();
    flow.onShareOpen.mockResolvedValue(unlisten);

    const dispose = mountShareEvents();
    await Promise.resolve();
    dispose();

    expect(unlisten).toHaveBeenCalledOnce();
  });

  it('unlistens a registration that lands after its effect unmounted', async () => {
    const unlisten = vi.fn();
    let resolveRegistration: (unlisten: () => void) => void = () => undefined;
    flow.onShareOpen.mockImplementation(
      () =>
        new Promise<(unlisten: () => void) => void>((resolve) => {
          resolveRegistration = resolve;
        }),
    );

    const dispose = mountShareEvents();
    dispose();
    resolveRegistration(unlisten);
    await Promise.resolve();

    expect(unlisten).toHaveBeenCalledOnce();
  });
});
