// SPDX-License-Identifier: GPL-3.0-only
// The sweep dialog's state: the sender address the row menu's Clean up
// entry named, standing until the dialog closes. Pure client-side view
// state, the swipe toast's own shape: the list pane mounts the dialog and
// the menu opens it through this store.

import { create } from 'zustand';

interface SweepStoreState {
  /** The sender the dialog sweeps; null while the dialog is closed. */
  sender: string | null;
  /** Opens the dialog over one sender address. */
  open: (sender: string) => void;
  /** Closes the dialog. */
  close: () => void;
}

export const useSweepStore = create<SweepStoreState>((set) => ({
  sender: null,
  open: (sender) => set({ sender }),
  close: () => set({ sender: null }),
}));