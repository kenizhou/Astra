// SPDX-License-Identifier: GPL-3.0-only
// The list filter's state: the tab the message list is showing. Pure
// client-side view state, exactly like the row selection it sits beside;
// the folder switch resets it to All (kylins' behaviour).

import { create } from 'zustand';

import type { ListFilter } from '@/components/message-list/listFilter';

interface ListFilterStoreState {
  /** The tab the list is showing; All until a toggle names one. */
  filter: ListFilter;
  /** The category the list narrows to, ANDed onto the tab; null for none. */
  categoryFilter: string | null;
  /** Records the tab the toolbar chose. */
  setFilter: (filter: ListFilter) => void;
  /** Records the category the picker chose; null clears the narrowing. */
  setCategoryFilter: (categoryId: string | null) => void;
  /** Back to All and no category: the folder switch's reset. */
  reset: () => void;
}

export const useListFilterStore = create<ListFilterStoreState>((set) => ({
  filter: 'all',
  categoryFilter: null,
  setFilter: (filter) => set({ filter }),
  setCategoryFilter: (categoryFilter) => set({ categoryFilter }),
  reset: () => set({ filter: 'all', categoryFilter: null }),
}));
