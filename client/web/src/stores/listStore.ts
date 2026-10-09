// SPDX-License-Identifier: GPL-3.0-only
// The message list's state: the last mailbox-list snapshot's rows, the
// grouping mode, and the visible window they were built against. The row
// selection is local to the surface: no intent exists for it, so the store
// holds what the user clicked and never writes it back.

import { create } from 'zustand';

import { sameRowKey } from '@/components/message-list/rowIdentity';
import type { MailboxListSnapshot, MessageRowView, RowKey, ViewMode } from '@/services/surfaces';

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

  applySnapshot: (snapshot) =>
    set((state) => ({
      rows: snapshot.rows ?? [],
      viewMode: snapshot.view_mode ?? 'flat',
      visibleLimit: snapshot.visible_limit ?? 0,
      selection: snapshot.selection ?? null,
      // The local selection survives a repull while its row still exists,
      // so acting on a row does not drop the highlight when the snapshot
      // returns.
      selectedKey:
        state.selectedKey !== null && (snapshot.rows ?? []).some((r) => sameRowKey(r.key, state.selectedKey))
          ? state.selectedKey
          : null,
    })),

  select: (key) => set({ selectedKey: key }),
}));
