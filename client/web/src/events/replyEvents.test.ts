// SPDX-License-Identifier: GPL-3.0-only
// The reply event mount: the host's watched reply batch routed into the same
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
  onReply: vi.fn(),
}));

vi.mock('./replyFlow', () => flow);

const record = vi.hoisted(() => vi.fn());

vi.mock('@/stores/notificationHistoryStore', () => ({
  useNotificationHistoryStore: { getState: () => ({ record }) },
}));

import { mountReplyEvents } from './replyEvents';
import type { NewMailBatchWire } from '@/stores/notificationHistoryStore';

const batch: NewMailBatchWire = {
  messages: [
    {
      account: 'work',
      key: 'k1',
      from: { name: 'Ada', email: 'ada@example.test' },
      subject: 'Re: Hello',
      preview: null,
    },
  ],
  overflow: 0,
};

/** Captures the handler the mount registered with the flow. */
function registeredHandler(): (batch: NewMailBatchWire) => void {
  expect(flow.onReply).toHaveBeenCalled();
  return flow.onReply.mock.calls[0]?.[0] as (batch: NewMailBatchWire) => void;
}

beforeEach(() => {
  flow.onReply.mockReset();
  record.mockReset();
});

describe('the reply event mount', () => {
  it('subscribes through the flow and records a batch into the history', async () => {
    flow.onReply.mockResolvedValue(() => undefined);

    const dispose = mountReplyEvents();
    await Promise.resolve();
    registeredHandler()(batch);

    expect(record).toHaveBeenCalledWith(batch);

    dispose();
  });

  it('disposes its subscription on unmount', async () => {
    const unlisten = vi.fn();
    flow.onReply.mockResolvedValue(unlisten);

    const dispose = mountReplyEvents();
    await Promise.resolve();
    dispose();

    expect(unlisten).toHaveBeenCalledOnce();
  });

  it('unlistens a registration that lands after its effect unmounted', async () => {
    const unlisten = vi.fn();
    let resolveRegistration: (unlisten: () => void) => void = () => undefined;
    flow.onReply.mockImplementation(
      () =>
        new Promise<(unlisten: () => void) => void>((resolve) => {
          resolveRegistration = resolve;
        }),
    );

    const dispose = mountReplyEvents();
    dispose();
    resolveRegistration(unlisten);
    await Promise.resolve();

    expect(unlisten).toHaveBeenCalledOnce();
  });
});