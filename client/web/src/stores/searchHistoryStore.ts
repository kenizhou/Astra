// SPDX-License-Identifier: GPL-3.0-only
// The session's search history: the queries the island's search submitted,
// newest first, capped so the dropdown stays a glance. Nothing persists;
// a fresh session starts with an empty list.

import { create } from 'zustand';

/** How many submitted queries the session keeps. */
export const SEARCH_HISTORY_LIMIT = 8;

interface SearchHistoryState {
  /** The submitted queries, newest first, no repeats. */
  queries: string[];
  /** Records a submitted query, moving a repeat back to the front. */
  record: (query: string) => void;
  /** Empties the list; the test seam. */
  clear: () => void;
}

export const useSearchHistoryStore = create<SearchHistoryState>((set) => ({
  queries: [],

  record: (query) =>
    set((state) => ({
      queries: [query, ...state.queries.filter((q) => q !== query)].slice(
        0,
        SEARCH_HISTORY_LIMIT,
      ),
    })),

  clear: () => set({ queries: [] }),
}));
