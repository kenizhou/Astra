// SPDX-License-Identifier: GPL-3.0-only
// The sweep dialog: the sender's matching rows in the loaded window only,
// the Archive default and the Move to Trash choice, the confirm batch
// dispatching the exact per-row intents for the matching rows and no
// others, the Cancel and Escape dismissals that dispatch nothing, and the
// empty match that disables the confirm with its reason. A mid-test store
// mutation under fake timers rides act() so the scheduler re-renders.
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { SweepDialog } from './SweepDialog';
import { useSweepStore } from './sweepStore';
import { useListStore } from '@/stores/listStore';
import type { MessageRowView, RowKey } from '@/services/surfaces';

/** One flat row, overridable per test. */
function row(key: string, over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key: { message: { account: 'work', key } },
    account_id: 'work',
    sender_name: 'Ada Lovelace',
    sender_address: 'ada@example.test',
    subject: `Subject ${key}`,
    sent_at: '2026-11-12T08:30:00Z',
    unread: false,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

const ada1 = row('a1', { subject: 'Quarterly report' });
const ada2 = row('a2', { subject: 'Digest of digests' });
const bob = row('b1', {
  sender_name: 'Bob',
  sender_address: 'bob@example.test',
  subject: 'Lunch plans',
});

/** Seeds the loaded window the dialog reads. */
function seedRows(rows: MessageRowView[]): void {
  useListStore.setState({ rows });
}

/** Opens the dialog over the sender, as the row menu entry does. */
function open(sender: string): void {
  useSweepStore.getState().open(sender);
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-11-12T12:00:00Z'));
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useSweepStore.setState({ sender: null });
  seedRows([]);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('the sweep dialog', () => {
  it('renders nothing while no sender stands', () => {
    render(<SweepDialog />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('names the sender and counts only the matching rows in the loaded folder', () => {
    seedRows([ada1, bob, ada2]);
    open('ada@example.test');
    render(<SweepDialog />);

    const dialog = screen.getByRole('dialog', { name: 'Clean up sender' });
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(within(dialog).getByText('ada@example.test')).toBeTruthy();
    expect(within(dialog).getByText('2 messages in this folder')).toBeTruthy();
    expect(within(dialog).getByText('Counts the loaded rows only.')).toBeTruthy();
    expect(within(dialog).queryByText('Lunch plans')).toBeNull();
  });

  it('previews the matching subjects, bounded, with the remainder named', () => {
    const many = Array.from({ length: 8 }, (_, i) => row(`m${i}`));
    seedRows([...many, bob]);
    open('ada@example.test');
    render(<SweepDialog />);

    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('Subject m0')).toBeTruthy();
    expect(within(dialog).getByText('Subject m4')).toBeTruthy();
    expect(within(dialog).queryByText('Subject m5')).toBeNull();
    expect(within(dialog).getByText('…and 3 more')).toBeTruthy();
  });

  it('defaults the action to Archive', () => {
    seedRows([ada1]);
    open('ada@example.test');
    render(<SweepDialog />);

    const group = screen.getByRole('group', { name: 'Action' });
    expect(within(group).getByRole('button', { name: 'Archive' }).getAttribute('aria-pressed')).toBe('true');
    expect(within(group).getByRole('button', { name: 'Move to Trash' }).getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByRole('button', { name: 'Archive 1 message' })).toBeTruthy();
  });

  it('confirm archives every matching row with the exact payload, and only those', () => {
    seedRows([ada1, bob, ada2]);
    open('ada@example.test');
    render(<SweepDialog />);

    fireEvent.click(screen.getByRole('button', { name: 'Archive 2 messages' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(ipc.dispatch).toHaveBeenCalledWith({ archive_message: { row: ada1.key } });
    expect(ipc.dispatch).toHaveBeenCalledWith({ archive_message: { row: ada2.key } });
    expect(useSweepStore.getState().sender).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('confirm moves every matching row to trash with the exact payload when that action stands', () => {
    seedRows([ada1, bob, ada2]);
    open('ada@example.test');
    render(<SweepDialog />);

    fireEvent.click(screen.getByRole('button', { name: 'Move to Trash' }));
    fireEvent.click(screen.getByRole('button', { name: 'Move 2 messages to Trash' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(ipc.dispatch).toHaveBeenCalledWith({ move_to_trash: { row: ada1.key } });
    expect(ipc.dispatch).toHaveBeenCalledWith({ move_to_trash: { row: ada2.key } });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('sweeps a thread row by its thread key', () => {
    const thread = row('t1', { key: { thread: { account: 'work', thread_id: 'th-1' } } as RowKey });
    seedRows([thread, bob]);
    open('ada@example.test');
    render(<SweepDialog />);

    fireEvent.click(screen.getByRole('button', { name: 'Archive 1 message' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({ archive_message: { row: thread.key } });
  });

  it('Cancel closes without dispatching', () => {
    seedRows([ada1, ada2]);
    open('ada@example.test');
    render(<SweepDialog />);

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(useSweepStore.getState().sender).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('Escape closes without dispatching', () => {
    seedRows([ada1, ada2]);
    open('ada@example.test');
    render(<SweepDialog />);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });

    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(useSweepStore.getState().sender).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('disables the confirm with its reason when no loaded row matches', () => {
    seedRows([bob]);
    open('ada@example.test');
    render(<SweepDialog />);

    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('0 messages in this folder')).toBeTruthy();
    expect(within(dialog).getByText('No loaded messages from this sender.')).toBeTruthy();
    const confirm = screen.getByRole('button', { name: 'Archive 0 messages' });
    expect(confirm.hasAttribute('disabled')).toBe(true);
    expect(confirm.getAttribute('title')).toBe('No loaded messages from this sender.');

    fireEvent.click(confirm);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('recomputes the match when the loaded window changes while the dialog stands', () => {
    seedRows([ada1, bob]);
    open('ada@example.test');
    render(<SweepDialog />);
    expect(screen.getByText('1 message in this folder')).toBeTruthy();

    act(() => {
      seedRows([ada1, bob, ada2]);
    });

    expect(screen.getByText('2 messages in this folder')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Archive 2 messages' }));
    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(ipc.dispatch).toHaveBeenCalledWith({ archive_message: { row: ada2.key } });
  });
});