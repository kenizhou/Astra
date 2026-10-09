// SPDX-License-Identifier: GPL-3.0-only
// The outbox panel: the queue's rows render with their state word, subject,
// and arrival, every state offers exactly the actions the engine accepts (a
// queued or refused send can go now or be withdrawn, an unconfirmed one
// offers only the user's answer), opening the panel asks the host to rebuild
// the queue, and the removal verbs mark their row so the send-result toast
// never reads the user's own hand as a delivery.
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import type { QueuedKey, QueuedRow } from '@/services/ipc';
import { useSendingStore } from '@/stores/sendingStore';
import { useUiStore } from '@/stores/uiStore';

import { OutboxPanel, OutboxStatus } from './OutboxPanel';
import { useSendResultStore } from './sendResult';

const work: QueuedKey = { account: 'work', op: 3 };

function row(state: QueuedRow['state'], overrides?: Partial<QueuedRow>): QueuedRow {
  return {
    queued: work,
    state,
    subject: 'Quarterly review',
    arrived_at: '2026-10-09T08:30:00Z',
    ...overrides,
  };
}

function reset(): void {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useSendingStore.getState().applySnapshot({ rows: [] });
  useSendResultStore.getState().dismiss();
  useSendResultStore.setState({ settled: [] });
  useUiStore.getState().setOutboxOpen(false);
}

beforeEach(reset);

afterEach(cleanup);

describe('the outbox status toggle', () => {
  it('opens and shuts the panel', () => {
    render(
      <>
        <OutboxStatus />
        <OutboxPanel />
      </>,
    );
    expect(screen.queryByRole('region', { name: 'Outbox' })).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Outbox' }));
    });
    expect(screen.getByRole('region', { name: 'Outbox' })).toBeTruthy();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Close outbox' }));
    });
    expect(screen.queryByRole('region', { name: 'Outbox' })).toBeNull();
  });

  it('carries the count of sends that have not gone out', () => {
    useSendingStore.getState().applySnapshot({ rows: [row('queued'), row('failed', { queued: { account: 'home', op: 1 } })] });
    render(<OutboxStatus />);

    expect(screen.getByRole('button', { name: 'Outbox 2' })).toBeTruthy();
  });
});

describe('the panel opening', () => {
  it('asks the host to rebuild the queue when it opens, once', () => {
    render(<OutboxPanel />);

    expect(ipc.dispatch).not.toHaveBeenCalled();

    act(() => {
      useUiStore.getState().setOutboxOpen(true);
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe('{"outbox":"show"}');

    ipc.dispatch.mockClear();
    act(() => {
      useSendingStore.getState().applySnapshot({ rows: [row('queued')] });
    });
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });
});

describe('the queue rows', () => {
  it('renders the state word, the subject, and the arrival instant', () => {
    useSendingStore.getState().applySnapshot({ rows: [row('unconfirmed')] });
    useUiStore.getState().setOutboxOpen(true);
    render(<OutboxPanel />);

    const list = screen.getByRole('region', { name: 'Outbox' });
    expect(within(list).getByText('Unconfirmed')).toBeTruthy();
    expect(within(list).getByText('Quarterly review')).toBeTruthy();
    expect(within(list).getByText('08:30 UTC')).toBeTruthy();
  });

  it('says so when a send carried no subject and arrived who knows when', () => {
    useSendingStore
      .getState()
      .applySnapshot({ rows: [row('failed', { subject: '', arrived_at: '' })] });
    useUiStore.getState().setOutboxOpen(true);
    render(<OutboxPanel />);

    const list = screen.getByRole('region', { name: 'Outbox' });
    expect(within(list).getByText('(no subject)')).toBeTruthy();
    expect(within(list).queryByText('UTC')).toBeNull();
  });

  it('offers an empty state when every send has gone', () => {
    useUiStore.getState().setOutboxOpen(true);
    render(<OutboxPanel />);

    expect(screen.getByText('No sends waiting')).toBeTruthy();
  });
});

describe('the row actions', () => {
  function openWith(rows: QueuedRow[]): void {
    useSendingStore.getState().applySnapshot({ rows });
    useUiStore.getState().setOutboxOpen(true);
    render(<OutboxPanel />);
    // The opening gesture's show is not what these tests pin.
    ipc.dispatch.mockClear();
  }

  it('offers send now and cancel on a queued send, with exact payloads', () => {
    openWith([row('queued')]);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send now' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"outbox":{"send_now":{"queued":{"account":"work","op":3}}}}',
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"outbox":{"cancel":{"queued":{"account":"work","op":3}}}}',
    );
  });

  it('offers send again and cancel on a send the server refused', () => {
    openWith([row('failed')]);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send again' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"outbox":{"send_now":{"queued":{"account":"work","op":3}}}}',
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"outbox":{"cancel":{"queued":{"account":"work","op":3}}}}',
    );
  });

  it('offers only the user answer on an unconfirmed send, with exact payloads', () => {
    openWith([row('unconfirmed')]);

    expect(screen.queryByRole('button', { name: 'Send now' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Cancel' })).toBeNull();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Mark as sent' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"outbox":{"confirm_sent":{"queued":{"account":"work","op":3}}}}',
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Send again' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"outbox":{"confirm_not_sent":{"queued":{"account":"work","op":3}}}}',
    );
  });

  it('offers nothing on a send that is in flight', () => {
    openWith([row('sending')]);

    expect(screen.queryByRole('button', { name: 'Send now' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Send again' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Cancel' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Mark as sent' })).toBeNull();
  });

  it('marks a withdrawn or settled row so the toast never calls it a delivery', () => {
    openWith([row('queued'), row('unconfirmed', { queued: { account: 'work', op: 7 } })]);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
      fireEvent.click(screen.getByRole('button', { name: 'Mark as sent' }));
    });

    expect(useSendResultStore.getState().settled).toEqual(['work/3', 'work/7']);
  });
});
