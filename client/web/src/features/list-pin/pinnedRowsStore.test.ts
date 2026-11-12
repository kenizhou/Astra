// SPDX-License-Identifier: GPL-3.0-only
// The pin set store: the optimistic toggle, isPinned, the per-account
// keying the rowKeyId strings carry, the device-local persistence (a
// malformed blob reads as empty, non-string entries drop), and the storage
// failure that is logged, never thrown.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { usePinnedRowsStore } from './pinnedRowsStore';
import { rowKeyId } from '@/components/message-list/rowIdentity';

const STORAGE_KEY = 'astra.pinnedRows';

const workKey = rowKeyId({ message: { account: 'work', key: 'k1' } });
const homeKey = rowKeyId({ message: { account: 'home', key: 'k1' } });
const threadKey = rowKeyId({ thread: { account: 'work', thread_id: 'th-1' } });

beforeEach(() => {
  localStorage.clear();
  usePinnedRowsStore.setState({ pinned: new Set() });
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe('the pinned rows store', () => {
  it('pins an unpinned key and reports it pinned', () => {
    expect(usePinnedRowsStore.getState().isPinned(workKey)).toBe(false);
    usePinnedRowsStore.getState().toggle(workKey);
    expect(usePinnedRowsStore.getState().isPinned(workKey)).toBe(true);
  });

  it('unpins a pinned key on the second toggle', () => {
    const store = usePinnedRowsStore.getState();
    store.toggle(workKey);
    store.toggle(workKey);
    expect(usePinnedRowsStore.getState().isPinned(workKey)).toBe(false);
  });

  it('scopes pins per account: the same key in two accounts stands apart', () => {
    usePinnedRowsStore.getState().toggle(workKey);
    expect(usePinnedRowsStore.getState().isPinned(workKey)).toBe(true);
    expect(usePinnedRowsStore.getState().isPinned(homeKey)).toBe(false);

    usePinnedRowsStore.getState().toggle(homeKey);
    expect(usePinnedRowsStore.getState().isPinned(workKey)).toBe(true);
    expect(usePinnedRowsStore.getState().isPinned(homeKey)).toBe(true);
  });

  it('keeps message and thread identities apart', () => {
    usePinnedRowsStore.getState().toggle(threadKey);
    expect(usePinnedRowsStore.getState().isPinned(threadKey)).toBe(true);
    expect(usePinnedRowsStore.getState().isPinned(workKey)).toBe(false);
  });

  it('persists every toggle to localStorage', () => {
    usePinnedRowsStore.getState().toggle(workKey);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual([workKey]);

    usePinnedRowsStore.getState().toggle(threadKey);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual([workKey, threadKey]);

    usePinnedRowsStore.getState().toggle(workKey);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual([threadKey]);
  });

  it('hydrates from localStorage on load', async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([workKey, threadKey]));
    vi.resetModules();
    const fresh = await import('./pinnedRowsStore');
    expect(fresh.usePinnedRowsStore.getState().isPinned(workKey)).toBe(true);
    expect(fresh.usePinnedRowsStore.getState().isPinned(threadKey)).toBe(true);
    expect(fresh.usePinnedRowsStore.getState().isPinned(homeKey)).toBe(false);
  });

  it('reads a malformed blob as empty', async () => {
    localStorage.setItem(STORAGE_KEY, '{not json');
    vi.resetModules();
    const fresh = await import('./pinnedRowsStore');
    expect(fresh.usePinnedRowsStore.getState().pinned.size).toBe(0);
  });

  it('reads a non-array blob as empty', async () => {
    localStorage.setItem(STORAGE_KEY, '{"pinned":[]}');
    vi.resetModules();
    const fresh = await import('./pinnedRowsStore');
    expect(fresh.usePinnedRowsStore.getState().pinned.size).toBe(0);
  });

  it('drops the malformed entries and keeps the well-formed ones', async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify([workKey, null, 42, '', { id: 'x' }, threadKey]),
    );
    vi.resetModules();
    const fresh = await import('./pinnedRowsStore');
    expect([...fresh.usePinnedRowsStore.getState().pinned]).toEqual([workKey, threadKey]);
  });

  it('logs a storage failure and never throws; the toggle still lands', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('quota');
    });

    usePinnedRowsStore.getState().toggle(workKey);

    expect(usePinnedRowsStore.getState().isPinned(workKey)).toBe(true);
    expect(error).toHaveBeenCalled();
  });
});
