// SPDX-License-Identifier: GPL-3.0-only
// The list filter's state: the tab the message list is showing. Pure
// client-side view state, exactly like the row selection it sits beside;
// the folder switch resets it to All (kylins' behaviour).

import { create } from 'zustand';

import type { ListFilter } from '@/components/message-list/listFilter';

interface ListFilterStoreState {
  /** The tab the list is showing; All until a toggle names one. */
  filter: ListFilter;
  /** Records the tab the toolbar chose. */
  setFilter: (filter: ListFilter) => void;
  /** Back to All: the folder switch's reset. */
  reset: () => void;
}

export const useListFilterStore = create<ListFilterStoreState>((set) => ({
  filter: 'all',
  setFilter: (filter) => set({ filter }),
  reset: () => set({ filter: 'all' }),
}));
