// SPDX-License-Identifier: GPL-3.0-only
// The outbox queue view: the account's rows render with their state word,
// subject, and arrival, every state offers exactly the actions the engine
// accepts (a queued or refused send can go now or be withdrawn, an
// unconfirmed one offers only the user's answer), and the removal verbs mark
// their row so the send-result toast never reads the user's own hand as a
// delivery. The payloads pinned here are the ones the floating panel pinned
// before the queue moved into the list pane.
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import type { QueuedKey, QueuedRow } from '@/services/ipc';
import type { HeldSend } from '@/services/sending';
import { useSendingStore } from '@/stores/sendingStore';

import { OutboxQueue } from './OutboxQueue';
import { SendResultToast } from './SendResultToast';
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
  useSendResultStore.setState({ settled: [], answered: [] });
}

beforeEach(reset);

afterEach(cleanup);

describe('the queue rows', () => {
  it('renders only the account the view leads, with state, subject, and arrival', () => {
    useSendingStore.getState().applySnapshot({
      rows: [row('unconfirmed'), row('queued', { queued: { account: 'home', op: 1 }, subject: 'Off shift' })],
    });
    render(<OutboxQueue account="work" />);

    const list = screen.getByRole('list', { name: 'Outbox' });
    expect(within(list).getByText('Unconfirmed')).toBeTruthy();
    expect(within(list).getByText('Quarterly review')).toBeTruthy();
    expect(within(list).getByText('08:30 UTC')).toBeTruthy();
    // The other account's send belongs to its own outbox row.
    expect(within(list).queryByText('Off shift')).toBeNull();
  });

  it('says so when a send carried no subject and arrived who knows when', () => {
    useSendingStore
      .getState()
      .applySnapshot({ rows: [row('failed', { subject: '', arrived_at: '' })] });
    render(<OutboxQueue account="work" />);

    const list = screen.getByRole('list', { name: 'Outbox' });
    expect(within(list).getByText('(no subject)')).toBeTruthy();
    expect(within(list).queryByText('UTC')).toBeNull();
  });

  it('offers an empty state when every send has gone', () => {
    render(<OutboxQueue account="work" />);

    expect(screen.getByText('No sends waiting')).toBeTruthy();
    expect(screen.queryByRole('list', { name: 'Outbox' })).toBeNull();
  });
});

describe('the row actions', () => {
  function openWith(rows: QueuedRow[]): void {
    useSendingStore.getState().applySnapshot({ rows });
    render(<OutboxQueue account="work" />);
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

  it('takes the settled marks back when the host refuses the removals', async () => {
    openWith([row('queued'), row('unconfirmed', { queued: { account: 'work', op: 7 } })]);
    ipc.dispatch.mockRejectedValue(new Error('the host is unreachable'));

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
      fireEvent.click(screen.getByRole('button', { name: 'Mark as sent' }));
    });
    await act(async () => {
      await Promise.resolve();
    });

    // The actions never reached the queue, so the sends' later genuine
    // deliveries still deserve their toast.
    expect(useSendResultStore.getState().settled).toEqual([]);
  });

  it('clears the settled mark when the engine refuses the removal in flight, and the later delivery toasts', async () => {
    useSendingStore.getState().applySnapshot({ rows: [row('queued')] });
    render(
      <>
        <OutboxQueue account="work" />
        <SendResultToast />
      </>,
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    });
    await act(async () => {
      await Promise.resolve();
    });
    // The engine answered; the mark stands until a snapshot judges it.
    expect(useSendResultStore.getState().settled).toEqual(['work/3']);

    // The send went in flight under the click, so the engine refused and the
    // row never left; the snapshot that follows takes the mark back off.
    act(() => {
      useSendingStore.getState().applySnapshot({ rows: [row('sending')] });
    });
    expect(useSendResultStore.getState().settled).toEqual([]);

    act(() => {
      useSendingStore.getState().applySnapshot({ rows: [] });
    });
    expect(screen.getByRole('status').textContent).toBe('Message sent');
  });
});

describe('the held rows', () => {
  function held(state: HeldSend['state']): HeldSend {
    return {
      hold: 'hold-7',
      account: 'work',
      subject: 'Held report',
      plugin: 'dev.astra.crypto',
      reason: 'no ready key for ada@example.test',
      state,
    };
  }

  it('renders a checking hold with the plugin named and no actions', () => {
    useSendingStore.getState().applySnapshot({ rows: [], held: [held('checking')] });
    render(<OutboxQueue account="work" />);
    expect(screen.getByText('Checking')).toBeTruthy();
    expect(screen.getByText('Waiting on dev.astra.crypto')).toBeTruthy();
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('renders a needs_answer hold with Send anyway and Deny', () => {
    useSendingStore.getState().applySnapshot({ rows: [], held: [held('needs_answer')] });
    render(<OutboxQueue account="work" />);
    expect(screen.getByText('dev.astra.crypto: no ready key for ada@example.test')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Send anyway' }));
    expect(ipc.dispatch).toHaveBeenCalledWith({
      outbox: { answer_gate: { hold: 'hold-7', send_anyway: true } },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Deny' }));
    expect(ipc.dispatch).toHaveBeenCalledWith({
      outbox: { answer_gate: { hold: 'hold-7', send_anyway: false } },
    });
  });

  it('renders a blocked hold with Retry and Withdraw', () => {
    useSendingStore.getState().applySnapshot({ rows: [], held: [held('blocked')] });
    render(<OutboxQueue account="work" />);
    expect(screen.getByText('Held')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(ipc.dispatch).toHaveBeenCalledWith({ outbox: { retry_held: { hold: 'hold-7' } } });
    fireEvent.click(screen.getByRole('button', { name: 'Withdraw' }));
    expect(ipc.dispatch).toHaveBeenCalledWith({ outbox: { withdraw_held: { hold: 'hold-7' } } });
  });

  it('shows the empty state only when neither leg has a row', () => {
    useSendingStore.getState().applySnapshot({ rows: [], held: [held('blocked')] });
    render(<OutboxQueue account="work" />);
    expect(screen.queryByText('No sends waiting')).toBeNull();
    expect(screen.getByText('Held report')).toBeTruthy();
  });
});
