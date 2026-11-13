// SPDX-License-Identifier: GPL-3.0-only
// The device-local pinned searches: named queries saved for later, persisted
// to localStorage the way the quick steps are. Only the query string is saved,
// never the scope: the scope is session state (search.md rule 7), so a saved
// search re-runs exactly as a typed one does. The list is capped at twenty,
// malformed entries drop on parse, and a storage failure is logged, never
// thrown. The panel gate rides here too, the quick steps store's own shape,
// so the title bar and the palette open the panel through the store.

import { create } from 'zustand';

/** How many searches the list keeps; the island disables save at the limit. */
export const MAX_PINNED_SEARCHES = 20;

const STORAGE_KEY = 'astra.pinnedSearches';

/** One saved search: a name and the query alone, never a scope. */
export interface PinnedSearch {
  id: string;
  name: string;
  query: string;
}

/** Parses one persisted entry; null when it is not well-formed. Any extra
 *  field (a scope, say) drops away: the query alone is kept. */
function parseSearch(value: unknown): PinnedSearch | null {
  if (typeof value !== 'object' || value === null) {
    return null;
  }
  const { id, name, query } = value as Record<string, unknown>;
  if (typeof id !== 'string' || id === '') {
    return null;
  }
  if (typeof name !== 'string' || name.trim() === '') {
    return null;
  }
  if (typeof query !== 'string' || query.trim() === '') {
    return null;
  }
  return { id, name, query };
}

/** Parses the persisted blob defensively; malformed data reads as empty. */
function readSearches(): PinnedSearch[] {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (raw === null) {
    return [];
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed
      .map(parseSearch)
      .filter((search): search is PinnedSearch => search !== null)
      .slice(0, MAX_PINNED_SEARCHES);
  } catch {
    return [];
  }
}

/** Persists the list; a storage failure is logged, never thrown. */
function persist(searches: PinnedSearch[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(searches));
  } catch (err) {
    console.error('pinned-searches: the searches could not be persisted', err);
  }
}

interface PinnedSearchesState {
  /** The saved searches, in the order they were saved. */
  searches: PinnedSearch[];
  /** Whether the panel stands. */
  panelOpen: boolean;
  /** Saves the query under the name; a blank query or name is refused, an
   *  identical query and name is idempotent, and the list caps at the limit. */
  save: (name: string, query: string) => void;
  /** Renames one search by id; a blank name or an absent id changes nothing. */
  rename: (id: string, name: string) => void;
  /** Removes one search by id; an absent id changes nothing. */
  remove: (id: string) => void;
  /** Opens the panel. */
  openPanel: () => void;
  /** Closes the panel. */
  closePanel: () => void;
}

export const usePinnedSearchesStore = create<PinnedSearchesState>((set) => ({
  searches: readSearches(),
  panelOpen: false,

  save: (name, query) =>
    set((state) => {
      const trimmedName = name.trim();
      const trimmedQuery = query.trim();
      if (trimmedName === '' || trimmedQuery === '') {
        return state;
      }
      const duplicate = state.searches.some(
        (existing) => existing.name === trimmedName && existing.query === trimmedQuery,
      );
      if (duplicate) {
        return state;
      }
      const entry: PinnedSearch = {
        id: newPinnedSearchId(),
        name: trimmedName,
        query: trimmedQuery,
      };
      // The island gates save at the limit; the clamp is the last line for
      // a hand-edited blob or a stale surface.
      const searches = [...state.searches, entry].slice(0, MAX_PINNED_SEARCHES);
      persist(searches);
      return { searches };
    }),

  rename: (id, name) =>
    set((state) => {
      const trimmedName = name.trim();
      if (trimmedName === '' || !state.searches.some((search) => search.id === id)) {
        return state;
      }
      const searches = state.searches.map((search) =>
        search.id === id ? { ...search, name: trimmedName } : search,
      );
      persist(searches);
      return { searches };
    }),

  remove: (id) =>
    set((state) => {
      const searches = state.searches.filter((search) => search.id !== id);
      persist(searches);
      return { searches };
    }),

  openPanel: () => set({ panelOpen: true }),
  closePanel: () => set({ panelOpen: false }),
}));

/** A fresh unique search id. */
export function newPinnedSearchId(): string {
  return crypto.randomUUID();
}
