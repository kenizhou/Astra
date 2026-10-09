// SPDX-License-Identifier: GPL-3.0-only
// The compose-request store: the composition a reply or forward opened, as
// the composer hydrates from it. The snapshot is immutable; a null pull
// means no request is pending, which is how a composer closes on nothing
// stale.

import { create } from 'zustand';

import type { ComposeRequestSnapshot } from '@/services/ipc';

interface ComposeRequestStoreState {
  /** The last published compose request; null when none is pending. */
  snapshot: ComposeRequestSnapshot | null;
  /** Applies a pulled snapshot. */
  applySnapshot: (snapshot: ComposeRequestSnapshot | null) => void;
}

export const useComposeRequestStore = create<ComposeRequestStoreState>((set) => ({
  snapshot: null,

  applySnapshot: (snapshot) => set({ snapshot }),
}));
