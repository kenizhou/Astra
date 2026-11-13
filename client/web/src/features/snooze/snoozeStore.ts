// SPDX-License-Identifier: GPL-3.0-only
// The snooze dialog's state: the message the row menu's Snooze entry named,
// standing until the dialog closes. Pure client-side view state, the sweep
// store's own shape: the list pane mounts the dialog and the menu opens it
// through this store.

import { create } from 'zustand';

import type { MessageRef } from '@/services/ipc';

interface SnoozeStoreState {
  /** The message the dialog snoozes; null while the dialog is closed. */
  target: MessageRef | null;
  /** Opens the dialog over one message. */
  open: (target: MessageRef) => void;
  /** Closes the dialog. */
  close: () => void;
}

export const useSnoozeStore = create<SnoozeStoreState>((set) => ({
  target: null,
  open: (target) => set({ target }),
  close: () => set({ target: null }),
}));