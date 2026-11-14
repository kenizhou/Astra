// SPDX-License-Identifier: GPL-3.0-only
// The category definitions snapshot: the defs the manager edits and the row
// badges and Categorize submenu resolve, loaded once from the host and
// reloaded after every mutation. Pure client-side view state over a local
// overlay (the snooze store's shape): the list pane mounts the loader and the
// dialog, the menu opens the manager through this store. The revision bumps
// on every load so a subscriber can tell one snapshot from the next.

import { create } from 'zustand';

import { listCategories, type CategoryDef } from '@/services/categories';

interface CategoriesStoreState {
  /** The definitions, creation order; empty until the first load lands. */
  defs: CategoryDef[];
  /** Bumped on every load: one snapshot's distinguisher from the next. */
  revision: number;
  /** Whether the first load has landed (the loader runs once). */
  loaded: boolean;
  /** Whether the manager dialog stands open. */
  dialogOpen: boolean;
  /** Pulls the definitions from the host and bumps the revision. */
  loadCategories: () => Promise<void>;
  /** Opens the manager dialog. */
  open: () => void;
  /** Closes the manager dialog. */
  close: () => void;
}

export const useCategoriesStore = create<CategoriesStoreState>((set) => ({
  defs: [],
  revision: 0,
  loaded: false,
  dialogOpen: false,
  loadCategories: async () => {
    try {
      const defs = await listCategories();
      set((state) => ({ defs, loaded: true, revision: state.revision + 1 }));
    } catch {
      // A failed defs pull leaves the snapshot empty: the pane still renders,
      // the badges and picker simply resolve nothing until a later load lands.
      set({ loaded: true });
    }
  },
  open: () => set({ dialogOpen: true }),
  close: () => set({ dialogOpen: false }),
}));