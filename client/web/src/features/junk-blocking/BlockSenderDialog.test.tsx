// SPDX-License-Identifier: GPL-3.0-only
// The block-sender dialog: the block dispatch, the optional report leg
// defaulted on, the report failure that never undoes the block, the
// canReport gate that hides the checkbox, and the Cancel/Escape dismissals
// that dispatch nothing. A mid-test store mutation under fake timers rides
// act() so the scheduler re-renders.
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { BlockSenderDialog } from './BlockSenderDialog';
import { useBlockSenderStore, type BlockTarget } from './blockSenderStore';
import type { RowRef } from '@/services/ipc';

const row: RowRef = { message: { account: 'work', key: 'k1' } };

/** Opens the dialog over one sender, as the row menu entry does. */
function open(over: Partial<BlockTarget> = {}): void {
  useBlockSenderStore.getState().open({
    address: 'ada@example.test',
    row,
    canReport: true,
    ...over,
  });
}

/** The dispatch payloads, in order, as JSON. */
function dispatched(): string[] {
  return ipc.dispatch.mock.calls.map((call) => JSON.stringify(call[0]));
}

const BLOCK = '{"settings":{"block_sender":{"address":"ada@example.test"}}}';
const REPORT = `{"mark_junk":{"row":${JSON.stringify(row)}}}`;

beforeEach(() => {
  vi.useFakeTimers();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useBlockSenderStore.setState({ target: null });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('the block sender dialog', () => {
  it('renders nothing while no sender stands', () => {
    render(<BlockSenderDialog />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('names the sender and defaults the report checkbox on', () => {
    open();
    render(<BlockSenderDialog />);
    const dialog = screen.getByRole('dialog', { name: 'Block sender' });
    expect(within(dialog).getByText('ada@example.test')).toBeTruthy();
    const checkbox = within(dialog).getByRole('checkbox', {
      name: 'Also report this conversation as junk',
    }) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
  });

  it('confirm blocks the sender and reports the conversation', () => {
    open();
    render(<BlockSenderDialog />);
    fireEvent.click(screen.getByRole('button', { name: 'Block sender' }));

    expect(dispatched()).toEqual([BLOCK, REPORT]);
    expect(useBlockSenderStore.getState().target).toBeNull();
  });

  it('unchecking the report leg blocks without reporting', () => {
    open();
    render(<BlockSenderDialog />);
    fireEvent.click(
      screen.getByRole('checkbox', { name: 'Also report this conversation as junk' }),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Block sender' }));

    expect(dispatched()).toEqual([BLOCK]);
  });

  it('hides the report leg where the account cannot report', () => {
    open({ canReport: false });
    render(<BlockSenderDialog />);
    expect(
      screen.queryByRole('checkbox', { name: 'Also report this conversation as junk' }),
    ).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Block sender' }));

    expect(dispatched()).toEqual([BLOCK]);
  });

  it('a report failure never undoes the block', () => {
    // The block resolves; the report rejects. The block is the durable act
    // and stands regardless; the dialog closes and the failure is logged.
    ipc.dispatch.mockImplementation((intent: unknown) => {
      const wire = JSON.stringify(intent);
      if (wire === BLOCK) {
        return Promise.resolve(undefined);
      }
      return Promise.reject(new Error('the report could not be queued'));
    });
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {});
    open();
    render(<BlockSenderDialog />);
    fireEvent.click(screen.getByRole('button', { name: 'Block sender' }));

    expect(dispatched()).toEqual([BLOCK, REPORT]);
    expect(useBlockSenderStore.getState().target).toBeNull();
    logged.mockRestore();
  });

  it('Cancel dispatches nothing and closes', () => {
    open();
    render(<BlockSenderDialog />);
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(useBlockSenderStore.getState().target).toBeNull();
  });

  it('Escape dispatches nothing and closes', () => {
    open();
    render(<BlockSenderDialog />);
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });

    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(useBlockSenderStore.getState().target).toBeNull();
  });

  it('a fresh open starts on the report default again', () => {
    open();
    const { unmount } = render(<BlockSenderDialog />);
    fireEvent.click(
      screen.getByRole('checkbox', { name: 'Also report this conversation as junk' }),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    unmount();

    act(() => {
      open({ address: 'bob@example.test' });
    });
    render(<BlockSenderDialog />);
    const checkbox = screen.getByRole('checkbox', {
      name: 'Also report this conversation as junk',
    }) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);
  });
});