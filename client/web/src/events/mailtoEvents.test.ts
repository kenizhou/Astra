// SPDX-License-Identifier: GPL-3.0-only
// The mailto event mount: the tested flow subscription routed into the
// composer's mailto open path, with the dispose race a StrictMode double
// mount runs (a registration that resolves after its effect unmounted
// unlistens itself instead of leaking a duplicate).
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: the mount subscribes only inside Tauri, so the jsdom
  // test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const flow = vi.hoisted(() => ({
  onMailtoOpen: vi.fn(),
  openMailtoComposer: vi.fn(),
}));

vi.mock('@/features/composer/mailtoOpen', () => flow);

import { mountMailtoEvents } from './mailtoEvents';
import type { MailtoOpen } from '@/features/composer/composerStore';

const open: MailtoOpen = {
  from: 'work',
  to: 'ada@example.test',
  cc: '',
  bcc: '',
  subject: 'Lunch',
  body: 'Noon?',
};

/** Captures the handler the mount registered with the flow. */
function registeredHandler(): (open: MailtoOpen) => void {
  expect(flow.onMailtoOpen).toHaveBeenCalled();
  return flow.onMailtoOpen.mock.calls[0]?.[0] as (open: MailtoOpen) => void;
}

beforeEach(() => {
  flow.onMailtoOpen.mockReset();
  flow.openMailtoComposer.mockReset();
});

describe('the mailto event mount', () => {
  it('subscribes through the flow and routes an open into the composer', async () => {
    flow.onMailtoOpen.mockResolvedValue(() => undefined);

    const dispose = mountMailtoEvents();
    await Promise.resolve();
    registeredHandler()(open);

    expect(flow.openMailtoComposer).toHaveBeenCalledWith(open);

    dispose();
  });

  it('disposes its subscription on unmount', async () => {
    const unlisten = vi.fn();
    flow.onMailtoOpen.mockResolvedValue(unlisten);

    const dispose = mountMailtoEvents();
    await Promise.resolve();
    dispose();

    expect(unlisten).toHaveBeenCalledOnce();
  });

  it('unlistens a registration that lands after its effect unmounted', async () => {
    const unlisten = vi.fn();
    let resolveRegistration: (unlisten: () => void) => void = () => undefined;
    flow.onMailtoOpen.mockImplementation(
      () =>
        new Promise<(unlisten: () => void) => void>((resolve) => {
          resolveRegistration = resolve;
        }),
    );

    const dispose = mountMailtoEvents();
    dispose();
    resolveRegistration(unlisten);
    await Promise.resolve();

    expect(unlisten).toHaveBeenCalledOnce();
  });
});
