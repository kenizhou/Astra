// SPDX-License-Identifier: GPL-3.0-only
// The message list's state: the last mailbox-list snapshot's rows, the
// grouping mode, the visible window they were built against, and the search
// state that supersedes them while a search runs. The row selection is
// local to the surface: no intent exists for it, so the store holds what
// the user clicked and never writes it back.

import { create } from 'zustand';

import { sameRowKey } from '@/components/message-list/rowIdentity';
import type {
  MailboxListSnapshot,
  MessageRowView,
  RowKey,
  SearchState,
  ViewMode,
} from '@/services/surfaces';

interface ListStoreState {
  /** The message rows in display order, exactly as the snapshot sent them. */
  rows: MessageRowView[];
  /** The mode the rows are grouped in; flat until a snapshot says otherwise. */
  viewMode: ViewMode;
  /** How many rows the visible window holds; the list's ShowMore compares. */
  visibleLimit: number;
  /** The selection the snapshot carries; the core stamps null for now. */
  selection: RowKey | null;
  /** The row the user clicked; local to this surface, never dispatched. */
  selectedKey: RowKey | null;
  /** The live search state; null at rest, while no search is running. */
  search: SearchState | null;
  /** Replaces the list view from a fresh mailbox-list snapshot. */
  applySnapshot: (snapshot: MailboxListSnapshot) => void;
  /** Records a local row selection the surface just drew. */
  select: (key: RowKey | null) => void;
}

export const useListStore = create<ListStoreState>((set) => ({
  rows: [],
  viewMode: 'flat',
  visibleLimit: 0,
  selection: null,
  selectedKey: null,
  search: null,

  applySnapshot: (snapshot) =>
    set((state) => ({
      rows: snapshot.rows ?? [],
      viewMode: snapshot.view_mode ?? 'flat',
      visibleLimit: snapshot.visible_limit ?? 0,
      selection: snapshot.selection ?? null,
      // The local selection survives a repull while its row still exists in
      // whichever row set the snapshot is showing, so acting on a row does
      // not drop the highlight when the snapshot returns.
      selectedKey: (() => {
        if (state.selectedKey === null) {
          return null;
        }
        const carried = snapshot.search?.rows ?? snapshot.rows ?? [];
        return carried.some((r) => sameRowKey(r.key, state.selectedKey)) ? state.selectedKey : null;
      })(),
      search: snapshot.search ?? null,
    })),

  select: (key) => set({ selectedKey: key }),
}));
