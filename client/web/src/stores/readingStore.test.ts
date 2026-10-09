// SPDX-License-Identifier: GPL-3.0-only
// The reading store: the snapshot a pull lands, and the show-remote-images
// flag that resets only when the open message changes, so a republish of the
// same message keeps the reader's opt-in while any new message starts gated.
import { beforeEach, describe, expect, it } from 'vitest';

import { useReadingStore } from './readingStore';
import type { ReadingSnapshot } from '@/services/ipc';

/** A snapshot of message `key`, overridable per test. */
function snapshot(key: string, over: Partial<ReadingSnapshot> = {}): ReadingSnapshot {
  return {
    message: { account: 'work', key },
    subject: 'Launch',
    from_name: 'Ada Lovelace',
    from_address: 'ada@example.test',
    sent_at: '2026-10-09T08:30:00Z',
    remote_image_count: 1,
    ...over,
  };
}

beforeEach(() => {
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
});

describe('the reading store', () => {
  it('holds the applied snapshot and clears on null', () => {
    useReadingStore.getState().applySnapshot(snapshot('m1'));
    expect(useReadingStore.getState().snapshot?.message.key).toBe('m1');

    useReadingStore.getState().applySnapshot(null);

    expect(useReadingStore.getState().snapshot).toBeNull();
  });

  it('resets the remote-images flag when the message changes', () => {
    useReadingStore.getState().applySnapshot(snapshot('m1'));
    useReadingStore.getState().setShowRemoteImages(true);

    useReadingStore.getState().applySnapshot(snapshot('m2'));

    expect(useReadingStore.getState().showRemoteImages).toBe(false);
  });

  it('resets the flag when the pane closes', () => {
    useReadingStore.getState().applySnapshot(snapshot('m1'));
    useReadingStore.getState().setShowRemoteImages(true);

    useReadingStore.getState().applySnapshot(null);

    expect(useReadingStore.getState().showRemoteImages).toBe(false);
  });

  it('keeps the flag over a republish of the same message', () => {
    useReadingStore.getState().applySnapshot(snapshot('m1', { subject: 'Launch' }));
    useReadingStore.getState().setShowRemoteImages(true);

    useReadingStore.getState().applySnapshot(snapshot('m1', { subject: 'Launch (updated)' }));

    expect(useReadingStore.getState().showRemoteImages).toBe(true);
    expect(useReadingStore.getState().snapshot?.subject).toBe('Launch (updated)');
  });

  it('treats the same key under another account as a different message', () => {
    useReadingStore.getState().applySnapshot(snapshot('m1'));
    useReadingStore.getState().setShowRemoteImages(true);

    useReadingStore.getState().applySnapshot({
      ...snapshot('m2'),
      message: { account: 'home', key: 'm1' },
    });

    expect(useReadingStore.getState().showRemoteImages).toBe(false);
  });
});
