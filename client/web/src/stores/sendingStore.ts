// SPDX-License-Identifier: GPL-3.0-only
// The sending store: the outbox panel's rows, every account's sends that have
// not gone out. Each pull replaces the whole queue, and an empty snapshot is a
// real state (every send has gone), so the panel renders it straight rather
// than clearing the store.

import { create } from 'zustand';

import type { SendingSnapshot } from '@/services/ipc';

interface SendingStoreState {
  /** The last published queue; empty when every send has gone. */
  snapshot: SendingSnapshot;
  /** Applies a pulled snapshot. */
  applySnapshot: (snapshot: SendingSnapshot) => void;
}

export const useSendingStore = create<SendingStoreState>((set) => ({
  snapshot: { rows: [] },

  applySnapshot: (snapshot) => set({ snapshot }),
}));
