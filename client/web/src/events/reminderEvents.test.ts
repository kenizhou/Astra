// SPDX-License-Identifier: GPL-3.0-only
// The reminder event mount: the host's woken batch routed into the same
// notification history store the new-mail mount feeds, with the dispose race
// a StrictMode double mount runs (a registration that resolves after its
// effect unmounted unlistens itself instead of leaking a duplicate).
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: the mount subscribes only inside Tauri, so the jsdom
  // test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const flow = vi.hoisted(() => ({
  onReminder: vi.fn(),
}));

vi.mock('./reminderFlow', () => flow);

const record = vi.hoisted(() => vi.fn());

vi.mock('@/stores/notificationHistoryStore', () => ({
  useNotificationHistoryStore: { getState: () => ({ record }) },
}));

import { mountReminderEvents } from './reminderEvents';
import type { NewMailBatchWire } from '@/stores/notificationHistoryStore';

const batch: NewMailBatchWire = {
  messages: [
    {
      account: 'work',
      key: 'k1',
      from: { name: 'Ada', email: 'ada@example.test' },
      subject: 'Hello',
      preview: null,
    },
  ],
  overflow: 0,
};

/** Captures the handler the mount registered with the flow. */
function registeredHandler(): (batch: NewMailBatchWire) => void {
  expect(flow.onReminder).toHaveBeenCalled();
  return flow.onReminder.mock.calls[0]?.[0] as (batch: NewMailBatchWire) => void;
}

beforeEach(() => {
  flow.onReminder.mockReset();
  record.mockReset();
});

describe('the reminder event mount', () => {
  it('subscribes through the flow and records a batch into the history', async () => {
    flow.onReminder.mockResolvedValue(() => undefined);

    const dispose = mountReminderEvents();
    await Promise.resolve();
    registeredHandler()(batch);

    expect(record).toHaveBeenCalledWith(batch);

    dispose();
  });

  it('disposes its subscription on unmount', async () => {
    const unlisten = vi.fn();
    flow.onReminder.mockResolvedValue(unlisten);

    const dispose = mountReminderEvents();
    await Promise.resolve();
    dispose();

    expect(unlisten).toHaveBeenCalledOnce();
  });

  it('unlistens a registration that lands after its effect unmounted', async () => {
    const unlisten = vi.fn();
    let resolveRegistration: (unlisten: () => void) => void = () => undefined;
    flow.onReminder.mockImplementation(
      () =>
        new Promise<(unlisten: () => void) => void>((resolve) => {
          resolveRegistration = resolve;
        }),
    );

    const dispose = mountReminderEvents();
    dispose();
    resolveRegistration(unlisten);
    await Promise.resolve();

    expect(unlisten).toHaveBeenCalledOnce();
  });
});
