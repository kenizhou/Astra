// SPDX-License-Identifier: GPL-3.0-only
// The ai chips store: the open message's smart-reply chips, pulled on open
// and re-pulled when the draft pool lands a row for that same message. The
// pull is fire and forget with a stale-resolution guard, so a slow answer
// for a message the pane has left behind never lands; a failure fails quiet,
// the strip stays absent, and the next open tries again. A dismissal clears
// the local chips beside its recorded write, so the strip hides at once.

import { create } from 'zustand';

import type { ChipView, MessageRef } from '@/services/ipc';
import { chipsFor, dismissChips } from '@/services/ai';

/** Whether two references name the same message. */
function sameMessage(a: MessageRef | null, b: MessageRef | null): boolean {
  if (a === null || b === null) {
    return a === b;
  }
  return a.account === b.account && a.key === b.key;
}

interface AiChipsStoreState {
  /** The message the held chips belong to; null while nothing is open. */
  message: MessageRef | null;
  /** The open message's chips, capped by the host's gate; empty with none. */
  chips: ChipView[];
  /** Opens the message: holds it and pulls its chips. */
  open: (message: MessageRef) => void;
  /** Re-pulls when the draft pool lands a row for the open message alone. */
  poolLanded: (message: MessageRef) => void;
  /** Dismisses the open message's chips: the recorded write and the clear. */
  dismiss: (message: MessageRef) => void;
  /** Clears the store: the pane holding it has closed or switched. */
  clear: () => void;
}

/** Pulls the message's chips into the store, ignoring a stale resolution. */
function pull(): void {
  const { message } = useAiChipsStore.getState();
  if (message === null) {
    return;
  }
  const pulled = message;
  void chipsFor(pulled).then(
    (chips) => {
      if (sameMessage(useAiChipsStore.getState().message, pulled)) {
        useAiChipsStore.setState({ chips });
      }
    },
    (err) => {
      console.error('ai chips: the pull failed', err);
    },
  );
}

export const useAiChipsStore = create<AiChipsStoreState>((set) => ({
  message: null,
  chips: [],

  open: (message) => {
    set({ message, chips: [] });
    pull();
  },

  poolLanded: (message) => {
    if (sameMessage(useAiChipsStore.getState().message, message)) {
      pull();
    }
  },

  dismiss: (message) => {
    if (!sameMessage(useAiChipsStore.getState().message, message)) {
      return;
    }
    set({ chips: [] });
    void dismissChips(message);
  },

  clear: () => set({ message: null, chips: [] }),
}));
