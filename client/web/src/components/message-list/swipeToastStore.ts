// SPDX-License-Identifier: GPL-3.0-only
// The swipe toast's state: the sentence naming the act that fired and the
// reverse intent its Undo dispatches, standing until the person undoes it or
// the moment passes.

import { create } from 'zustand';

import type { Intent } from '@/services/ipc';

interface SwipeToastState {
  note: string | null;
  undo: Intent | null;
  show: (note: string, undo: Intent | null) => void;
  dismiss: () => void;
}

export const useSwipeToastStore = create<SwipeToastState>((set) => ({
  note: null,
  undo: null,
  show: (note, undo) => set({ note, undo }),
  dismiss: () => set({ note: null, undo: null }),
}));
