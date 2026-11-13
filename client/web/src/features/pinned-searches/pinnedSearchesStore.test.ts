// SPDX-License-Identifier: GPL-3.0-only
// The pinned searches store: save (the blank-query refusal, the idempotent
// repeat, duplicate names under distinct ids), rename, remove, the ordered
// list, the twenty-search cap, the device-local persistence (a malformed blob
// reads as empty, malformed entries drop, a storage failure is logged never
// thrown), and the panel gate.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  MAX_PINNED_SEARCHES,
  usePinnedSearchesStore,
  type PinnedSearch,
} from './pinnedSearchesStore';

const STORAGE_KEY = 'astra.pinnedSearches';

/** One saved search, overridable per test. */
function saved(id: string, over: Partial<PinnedSearch> = {}): PinnedSearch {
  return { id, name: `Search ${id}`, query: 'from:' + id + '@example.test', ...over };
}

/** The stored ids, in order. */
function ids(): string[] {
  return usePinnedSearchesStore.getState().searches.map((s) => s.id);
}

beforeEach(() => {
  localStorage.clear();
  usePinnedSearchesStore.setState({ searches: [], panelOpen: false });
});

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe('the pinned searches store', () => {
  it('saves searches in order and keeps the list ordered', () => {
    const store = usePinnedSearchesStore.getState();
    store.save('Ada', 'from:ada');
    store.save('Monitor', 'monitor');
    store.save('Receipts', 'receipts');
    expect(usePinnedSearchesStore.getState().searches.map((s) => s.name)).toEqual([
      'Ada',
      'Monitor',
      'Receipts',
    ]);
    expect(usePinnedSearchesStore.getState().searches.every((s) => s.id !== '')).toBe(true);
  });

  it('refuses a blank query: there is nothing to pin', () => {
    const store = usePinnedSearchesStore.getState();
    store.save('Empty', '');
    store.save('Spaces', '   ');
    expect(usePinnedSearchesStore.getState().searches).toEqual([]);
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it('refuses a blank name', () => {
    const store = usePinnedSearchesStore.getState();
    store.save('', 'from:ada');
    store.save('   ', 'from:ada');
    expect(usePinnedSearchesStore.getState().searches).toEqual([]);
  });

  it('trims the name and the query on save', () => {
    usePinnedSearchesStore.getState().save('  Ada  ', '  from:ada  ');
    expect(usePinnedSearchesStore.getState().searches[0]).toMatchObject({
      name: 'Ada',
      query: 'from:ada',
    });
  });

  it('saves an identical query and name once: the repeat is idempotent', () => {
    const store = usePinnedSearchesStore.getState();
    store.save('Ada', 'from:ada');
    const before = usePinnedSearchesStore.getState().searches;
    store.save('Ada', 'from:ada');
    expect(usePinnedSearchesStore.getState().searches).toEqual(before);
    expect(ids()).toHaveLength(1);
  });

  it('allows duplicate names: the id is the identity', () => {
    const store = usePinnedSearchesStore.getState();
    store.save('Ada', 'from:ada');
    store.save('Ada', 'from:ada monitor');
    const searches = usePinnedSearchesStore.getState().searches;
    expect(searches).toHaveLength(2);
    expect(searches[0]?.id).not.toBe(searches[1]?.id);
    expect(searches.map((s) => s.query)).toEqual(['from:ada', 'from:ada monitor']);
  });

  it('keeps one query under two names as two entries', () => {
    const store = usePinnedSearchesStore.getState();
    store.save('Ada', 'from:ada');
    store.save('Ada again', 'from:ada');
    expect(usePinnedSearchesStore.getState().searches).toHaveLength(2);
  });

  it('renames in place, keeping the position and the query', () => {
    const store = usePinnedSearchesStore.getState();
    store.save('Ada', 'from:ada');
    store.save('Monitor', 'monitor');
    const firstId = ids()[0];
    store.rename(firstId as string, 'Ada Lovelace');
    expect(usePinnedSearchesStore.getState().searches[0]).toEqual({
      id: firstId,
      name: 'Ada Lovelace',
      query: 'from:ada',
    });
    expect(usePinnedSearchesStore.getState().searches[1]?.name).toBe('Monitor');
  });

  it('refuses a blank rename and changes nothing for an absent id', () => {
    const store = usePinnedSearchesStore.getState();
    store.save('Ada', 'from:ada');
    const before = usePinnedSearchesStore.getState().searches;
    store.rename(ids()[0] as string, '   ');
    store.rename('gone', 'Renamed');
    expect(usePinnedSearchesStore.getState().searches).toEqual(before);
  });

  it('removes by id; an absent id changes nothing', () => {
    const store = usePinnedSearchesStore.getState();
    store.save('Ada', 'from:ada');
    store.save('Monitor', 'monitor');
    const [first, second] = ids();
    store.remove(first as string);
    expect(ids()).toEqual([second]);
    store.remove('gone');
    expect(ids()).toEqual([second]);
  });

  it('caps the list at twenty, dropping the overflow on save', () => {
    const store = usePinnedSearchesStore.getState();
    for (let i = 0; i < MAX_PINNED_SEARCHES + 2; i += 1) {
      store.save(`Search ${i}`, `query:${i}`);
    }
    expect(usePinnedSearchesStore.getState().searches).toHaveLength(MAX_PINNED_SEARCHES);
    expect(usePinnedSearchesStore.getState().searches[0]?.query).toBe('query:0');
    expect(usePinnedSearchesStore.getState().searches.at(-1)?.query).toBe('query:19');
    expect(MAX_PINNED_SEARCHES).toBe(20);
  });

  it('still renames while the list stands at the cap', () => {
    const store = usePinnedSearchesStore.getState();
    for (let i = 0; i < MAX_PINNED_SEARCHES; i += 1) {
      store.save(`Search ${i}`, `query:${i}`);
    }
    const last = ids()[MAX_PINNED_SEARCHES - 1];
    store.rename(last as string, 'Renamed at the cap');
    expect(usePinnedSearchesStore.getState().searches).toHaveLength(MAX_PINNED_SEARCHES);
    expect(usePinnedSearchesStore.getState().searches.at(-1)?.name).toBe('Renamed at the cap');
  });

  it('persists every change to localStorage', () => {
    const store = usePinnedSearchesStore.getState();
    store.save('Ada', 'from:ada');
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(
      usePinnedSearchesStore.getState().searches,
    );

    store.rename(ids()[0] as string, 'Renamed');
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(
      usePinnedSearchesStore.getState().searches,
    );

    store.remove(ids()[0] as string);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual([]);
  });

  it('hydrates from localStorage on load', async () => {
    const stored = [saved('a', { name: 'Ada', query: 'from:ada' })];
    localStorage.setItem(STORAGE_KEY, JSON.stringify(stored));
    vi.resetModules();
    const fresh = await import('./pinnedSearchesStore');
    expect(fresh.usePinnedSearchesStore.getState().searches).toEqual(stored);
  });

  it('reads a malformed blob as empty', async () => {
    localStorage.setItem(STORAGE_KEY, '{not json');
    vi.resetModules();
    const fresh = await import('./pinnedSearchesStore');
    expect(fresh.usePinnedSearchesStore.getState().searches).toEqual([]);
  });

  it('reads a non-array blob as empty', async () => {
    localStorage.setItem(STORAGE_KEY, '{"searches":[]}');
    vi.resetModules();
    const fresh = await import('./pinnedSearchesStore');
    expect(fresh.usePinnedSearchesStore.getState().searches).toEqual([]);
  });

  it('drops the malformed entries and keeps the well-formed ones', async () => {
    const good = saved('good');
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify([
        good,
        null,
        42,
        { id: '', name: 'No id', query: 'from:ada' },
        { id: 'x', name: '  ', query: 'from:ada' },
        { id: 'x', name: 'Blank query', query: '   ' },
        { id: 'x', name: 'Missing query' },
        { id: 'x', name: 'Bad query', query: 7 },
      ]),
    );
    vi.resetModules();
    const fresh = await import('./pinnedSearchesStore');
    expect(fresh.usePinnedSearchesStore.getState().searches).toEqual([good]);
  });

  it('keeps the query alone when a persisted entry carries a scope', async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify([{ id: 'x', name: 'Narrowed', query: 'from:ada', scope: 'current_folder' }]),
    );
    vi.resetModules();
    const fresh = await import('./pinnedSearchesStore');
    expect(fresh.usePinnedSearchesStore.getState().searches).toEqual([
      { id: 'x', name: 'Narrowed', query: 'from:ada' },
    ]);
  });

  it('clamps an over-long persisted list to the cap on load', async () => {
    const stored = Array.from({ length: MAX_PINNED_SEARCHES + 3 }, (_, i) => saved(`s${i}`));
    localStorage.setItem(STORAGE_KEY, JSON.stringify(stored));
    vi.resetModules();
    const fresh = await import('./pinnedSearchesStore');
    expect(fresh.usePinnedSearchesStore.getState().searches).toHaveLength(MAX_PINNED_SEARCHES);
  });

  it('logs a storage failure and keeps the list in memory, never throws', () => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('quota exceeded');
    });
    expect(() => usePinnedSearchesStore.getState().save('Ada', 'from:ada')).not.toThrow();
    expect(usePinnedSearchesStore.getState().searches).toHaveLength(1);
    expect(logged).toHaveBeenCalled();
  });

  it('opens and closes the panel through the gate', () => {
    expect(usePinnedSearchesStore.getState().panelOpen).toBe(false);
    usePinnedSearchesStore.getState().openPanel();
    expect(usePinnedSearchesStore.getState().panelOpen).toBe(true);
    usePinnedSearchesStore.getState().closePanel();
    expect(usePinnedSearchesStore.getState().panelOpen).toBe(false);
  });
});
