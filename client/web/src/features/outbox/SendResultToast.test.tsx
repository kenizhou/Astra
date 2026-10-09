// SPDX-License-Identifier: GPL-3.0-only
// The send-result toast: a row that leaves the queue without this session's
// own hand removing it is a delivery, and the toast says the one honest word
// for it; a row the user withdrew or settled themselves stays quiet, and the
// toast leaves again after its moment.
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { QueuedKey, QueuedRow } from '@/services/ipc';
import { useSendingStore } from '@/stores/sendingStore';

import { SendResultToast } from './SendResultToast';
import { SEND_TOAST_MS, keyOf, useSendResultStore, vanishedKeys } from './sendResult';

function row(key: QueuedKey): QueuedRow {
  return {
    queued: key,
    state: 'queued',
    subject: 'Quarterly review',
    arrived_at: '2026-10-09T08:30:00Z',
  };
}

function queue(...rows: QueuedRow[]): void {
  act(() => {
    useSendingStore.getState().applySnapshot({ rows });
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  useSendingStore.getState().applySnapshot({ rows: [] });
  useSendResultStore.getState().dismiss();
  useSendResultStore.setState({ settled: [] });
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('which sends count as delivered', () => {
  it('names a queued send by its account and op', () => {
    expect(keyOf({ account: 'work', op: 3 })).toBe('work/3');
  });

  it('finds the sends that left the queue between two snapshots', () => {
    const before = [row({ account: 'work', op: 1 }), row({ account: 'work', op: 2 })];
    const after = [row({ account: 'work', op: 2 }), row({ account: 'home', op: 9 })];

    expect(vanishedKeys(before, after)).toEqual(['work/1']);
    expect(vanishedKeys(after, before)).toEqual(['home/9']);
    expect(vanishedKeys(before, before)).toEqual([]);
  });
});

describe('the toast on the sending signal', () => {
  it('stays quiet while sends only join the queue', () => {
    render(<SendResultToast />);
    queue(row({ account: 'work', op: 1 }));

    expect(screen.queryByRole('status')).toBeNull();
  });

  it('says the message was sent when a row leaves the queue on its own', () => {
    render(<SendResultToast />);
    queue(row({ account: 'work', op: 1 }));
    queue();

    expect(screen.getByRole('status').textContent).toBe('Message sent');
  });

  it('stays quiet when the user withdrew or settled the send themselves', () => {
    render(<SendResultToast />);
    const key = { account: 'work', op: 4 };
    queue(row(key));
    // The panel records its own removal verbs before the queue answers.
    act(() => {
      useSendResultStore.getState().noteUserSettled(key);
    });
    queue();

    expect(screen.queryByRole('status')).toBeNull();
  });

  it('toasts again for a later delivery after the first one left', () => {
    render(<SendResultToast />);
    queue(row({ account: 'work', op: 1 }));
    queue();
    expect(screen.getByRole('status')).toBeTruthy();

    act(() => {
      vi.advanceTimersByTime(SEND_TOAST_MS);
    });
    expect(screen.queryByRole('status')).toBeNull();

    queue(row({ account: 'work', op: 2 }));
    queue();
    expect(screen.getByRole('status').textContent).toBe('Message sent');
  });

  it('leaves by itself after its moment', () => {
    render(<SendResultToast />);
    queue(row({ account: 'work', op: 1 }));
    queue();

    act(() => {
      vi.advanceTimersByTime(SEND_TOAST_MS - 1);
    });
    expect(screen.getByRole('status')).toBeTruthy();

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.queryByRole('status')).toBeNull();
  });
});
