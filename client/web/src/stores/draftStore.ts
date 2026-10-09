// SPDX-License-Identifier: GPL-3.0-only
// The draft-status store: the composition whose save moved and how it ended,
// as the composer's quiet hint renders it. The snapshot names its composition,
// so the composer matches it against its own and ignores a stale one; whether
// to show the hint stays the composer's call, not the store's.

import { create } from 'zustand';

import type { DraftStatusSnapshot } from '@/services/ipc';

interface DraftStoreState {
  /** The last published save hint; null when no composition has one. */
  snapshot: DraftStatusSnapshot | null;
  /** Applies a pulled snapshot. */
  applySnapshot: (snapshot: DraftStatusSnapshot | null) => void;
}

export const useDraftStore = create<DraftStoreState>((set) => ({
  snapshot: null,

  applySnapshot: (snapshot) => set({ snapshot }),
}));
