// SPDX-License-Identifier: GPL-3.0-only
// The block-sender dialog's state: the sender the row menu's Block entry
// named, the row the optional report leg reports, and whether the account
// can report, standing until the dialog closes. Pure client-side view state,
// the sweep store's own shape: the list pane mounts the dialog and the menu
// opens it through this store. The blocklist itself is a preference the
// settings wire carries, never held here.

import { create } from 'zustand';

import type { RowRef } from '@/services/ipc';

/** One block ask: the sender to block, the row the report leg names, and
 *  whether the report leg is offered (the provider can report and the tree
 *  resolves a junk destination, the spam verb's own visibility rule). */
export interface BlockTarget {
  address: string;
  row: RowRef;
  canReport: boolean;
}

interface BlockSenderStoreState {
  /** The block ask the dialog answers; null while the dialog is closed. */
  target: BlockTarget | null;
  /** Opens the dialog over one sender and row. */
  open: (target: BlockTarget) => void;
  /** Closes the dialog. */
  close: () => void;
}

export const useBlockSenderStore = create<BlockSenderStoreState>((set) => ({
  target: null,
  open: (target) => set({ target }),
  close: () => set({ target: null }),
}));