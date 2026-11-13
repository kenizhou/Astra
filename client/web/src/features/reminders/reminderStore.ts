// SPDX-License-Identifier: GPL-3.0-only
// The reminder dialog's state: the message the row menu's Remind me entry
// named, standing until the dialog closes. Pure client-side view state, the
// snooze store's own shape: the list pane mounts the dialog and the menu
// opens it through this store.

import { create } from 'zustand';

import type { MessageRef } from '@/services/ipc';

interface ReminderStoreState {
  /** The message the dialog reminds about; null while the dialog is closed. */
  target: MessageRef | null;
  /** Opens the dialog over one message. */
  open: (target: MessageRef) => void;
  /** Closes the dialog. */
  close: () => void;
}

export const useReminderStore = create<ReminderStoreState>((set) => ({
  target: null,
  open: (target) => set({ target }),
  close: () => set({ target: null }),
}));
