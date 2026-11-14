// SPDX-License-Identifier: GPL-3.0-only
// The watch-replies store: the loader seeds the set from the listing (both
// the thread id and the representative member key), the row resolution flips
// on the watched state, and the toggle verbs call the host then refresh. The
// service is mocked whole so the store reads preset answers.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const service = vi.hoisted(() => ({
  watchSet: vi.fn(),
  watchClear: vi.fn(),
  watchList: vi.fn(),
}));

vi.mock('@/services/watchReplies', () => ({
  watchSet: service.watchSet,
  watchClear: service.watchClear,
  watchList: service.watchList,
}));

import { useWatchStore } from './watchStore';
import type { MessageRowView, RowKey } from '@/services/surfaces';

function row(key: RowKey): MessageRowView {
  return {
    key,
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'One',
    sent_at: '2026-10-09T08:30:00Z',
    unread: false,
    flagged: false,
    has_attachments: false,
  };
}

const messageRow = row({ message: { account: 'work', key: 'm1' } });
const threadRow = row({ thread: { account: 'work', thread_id: 'th-1' } });

const entry = {
  account: 'work',
  thread_id: 'th-1',
  member_key: 'm1',
  created_at: 1_000,
};

beforeEach(() => {
  service.watchSet.mockReset();
  service.watchClear.mockReset();
  service.watchList.mockReset();
  useWatchStore.setState({ watched: new Set(), loaded: false, revision: 0 });
});

describe('the watch-replies store', () => {
  it('seeds the set from the listing, marks loaded, and bumps the revision', async () => {
    service.watchList.mockResolvedValue([entry]);

    await useWatchStore.getState().loadWatches();

    const state = useWatchStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.revision).toBe(1);
    expect(state.watched.has('work:th-1')).toBe(true);
    expect(state.watched.has('work:m1')).toBe(true);
  });

  it('resolves a thread row by its thread id and a message row by the member key', async () => {
    service.watchList.mockResolvedValue([entry]);
    await useWatchStore.getState().loadWatches();

    expect(useWatchStore.getState().isWatched(threadRow)).toBe(true);
    expect(useWatchStore.getState().isWatched(messageRow)).toBe(true);
    expect(useWatchStore.getState().isWatched(row({ message: { account: 'work', key: 'other' } }))).toBe(false);
  });

  it('scopes the watched state to the account', async () => {
    service.watchList.mockResolvedValue([entry]);
    await useWatchStore.getState().loadWatches();

    expect(useWatchStore.getState().isWatched(row({ thread: { account: 'home', thread_id: 'th-1' } }))).toBe(false);
  });

  it('watches through the host then refreshes the set', async () => {
    service.watchSet.mockResolvedValue(true);
    service.watchList.mockResolvedValue([entry]);

    await useWatchStore.getState().watch(messageRow);

    expect(service.watchSet).toHaveBeenCalledWith(messageRow.key);
    expect(service.watchList).toHaveBeenCalledTimes(1);
    expect(useWatchStore.getState().isWatched(messageRow)).toBe(true);
  });

  it('unwatches through the host then refreshes the set', async () => {
    service.watchClear.mockResolvedValue(true);
    service.watchList.mockResolvedValue([]);

    await useWatchStore.getState().unwatch(messageRow);

    expect(service.watchClear).toHaveBeenCalledWith(messageRow.key);
    expect(useWatchStore.getState().isWatched(messageRow)).toBe(false);
  });

  it('a failed pull leaves the set empty and marks loaded', async () => {
    service.watchList.mockRejectedValue(new Error('the host is down'));

    await useWatchStore.getState().loadWatches();

    const state = useWatchStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.watched.size).toBe(0);
  });
});