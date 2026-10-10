// SPDX-License-Identifier: GPL-3.0-only
// The ai chips store: the pull an open drives, the re-pull a draft-pool
// landing drives for the open message alone, the local clear a dismissal
// makes beside its recorded write, and the stale-resolution guard that keeps
// a slow pull for a message the pane has left behind from ever landing.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ai = vi.hoisted(() => ({
  chipsFor: vi.fn(),
  dismissChips: vi.fn(),
}));

vi.mock('@/services/ai', () => ai);

import { useAiChipsStore } from './aiChipsStore';
import type { ChipView, MessageRef } from '@/services/ipc';

const work: MessageRef = { account: 'work', key: 'imap:v1:u1' };
const other: MessageRef = { account: 'work', key: 'imap:v1:u2' };
const chip: ChipView = { reply: 'Shipped.', preview: 'Shipped.' };

/** Flushes the microtasks the store's fire-and-forget pull rides. */
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  ai.chipsFor.mockReset();
  ai.dismissChips.mockReset();
  ai.chipsFor.mockResolvedValue([]);
  ai.dismissChips.mockResolvedValue(undefined);
  useAiChipsStore.setState({ message: null, chips: [] });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('the ai chips store', () => {
  it('opens a message by pulling and holding its chips', async () => {
    ai.chipsFor.mockResolvedValue([chip]);

    useAiChipsStore.getState().open(work);
    await settle();

    expect(ai.chipsFor).toHaveBeenCalledWith(work);
    expect(useAiChipsStore.getState().message).toEqual(work);
    expect(useAiChipsStore.getState().chips).toEqual([chip]);
  });

  it('never lands a pull for a message the pane has left behind', async () => {
    let resolveFirst: (chips: ChipView[]) => void = () => undefined;
    ai.chipsFor.mockImplementationOnce(
      () =>
        new Promise<ChipView[]>((resolve) => {
          resolveFirst = resolve;
        }),
    );

    useAiChipsStore.getState().open(work);
    useAiChipsStore.getState().open(other);
    await settle();
    resolveFirst([chip]);
    await settle();

    expect(useAiChipsStore.getState().message).toEqual(other);
    expect(useAiChipsStore.getState().chips).toEqual([]);
  });

  it('fails a pull quiet: no chips, the pane stays open to try again', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    ai.chipsFor.mockRejectedValue(new Error('the transport refused'));

    useAiChipsStore.getState().open(work);
    await settle();

    expect(useAiChipsStore.getState().message).toEqual(work);
    expect(useAiChipsStore.getState().chips).toEqual([]);
    expect(error).toHaveBeenCalledOnce();
  });

  it('re-pulls on a draft-pool landing for the open message alone', async () => {
    ai.chipsFor.mockResolvedValue([chip]);
    useAiChipsStore.getState().open(work);
    await settle();
    ai.chipsFor.mockClear();

    useAiChipsStore.getState().poolLanded(other);
    await settle();
    expect(ai.chipsFor).not.toHaveBeenCalled();

    useAiChipsStore.getState().poolLanded(work);
    await settle();
    expect(ai.chipsFor).toHaveBeenCalledWith(work);
    expect(useAiChipsStore.getState().chips).toEqual([chip]);
  });

  it('never re-lands a pull that was in flight across a dismissal', async () => {
    ai.chipsFor.mockResolvedValue([chip]);
    useAiChipsStore.getState().open(work);
    await settle();

    // A draft-pool landing starts a re-pull that stays in flight across the
    // reader's dismissal click.
    let resolveLanded: (chips: ChipView[]) => void = () => undefined;
    ai.chipsFor.mockImplementationOnce(
      () =>
        new Promise<ChipView[]>((resolve) => {
          resolveLanded = resolve;
        }),
    );
    useAiChipsStore.getState().poolLanded(work);

    useAiChipsStore.getState().dismiss(work);
    expect(useAiChipsStore.getState().chips).toEqual([]);

    resolveLanded([chip]);
    await settle();
    expect(
      useAiChipsStore.getState().chips,
      'the pre-dismissal pull cannot resurface what the reader put away',
    ).toEqual([]);
    expect(useAiChipsStore.getState().message).toEqual(work);

    // A later landing still re-pulls, and the durable marker keeps it empty.
    ai.chipsFor.mockResolvedValueOnce([]);
    useAiChipsStore.getState().poolLanded(work);
    await settle();
    expect(ai.chipsFor).toHaveBeenLastCalledWith(work);
    expect(useAiChipsStore.getState().chips).toEqual([]);
  });

  it('dismisses the open message by clearing locally and recording the write', async () => {
    ai.chipsFor.mockResolvedValue([chip]);
    useAiChipsStore.getState().open(work);
    await settle();

    useAiChipsStore.getState().dismiss(work);

    expect(ai.dismissChips).toHaveBeenCalledWith(work);
    expect(useAiChipsStore.getState().chips).toEqual([]);
    expect(useAiChipsStore.getState().message).toEqual(work);
  });

  it('dismisses nothing for a message that is not the open one', async () => {
    ai.chipsFor.mockResolvedValue([chip]);
    useAiChipsStore.getState().open(work);
    await settle();

    useAiChipsStore.getState().dismiss(other);

    expect(ai.dismissChips).not.toHaveBeenCalled();
    expect(useAiChipsStore.getState().chips).toEqual([chip]);
  });

  it('clears the message and its chips', async () => {
    useAiChipsStore.getState().open(work);
    await settle();

    useAiChipsStore.getState().clear();

    expect(useAiChipsStore.getState().message).toBeNull();
    expect(useAiChipsStore.getState().chips).toEqual([]);
  });
});
