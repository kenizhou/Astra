// SPDX-License-Identifier: GPL-3.0-only
// The shortcuts help's open state: a tiny standalone store the overlay reads,
// with module-scope verbs so non-React code (the keyboard opener, a future
// shortcut registry) can open, close, and toggle it without a hook.

import { create } from 'zustand';

interface ShortcutsHelpUiState {
  isOpen: boolean;
  open: () => void;
  close: () => void;
  toggle: () => void;
}

export const useShortcutsHelpStore = create<ShortcutsHelpUiState>((set) => ({
  isOpen: false,
  open: () => set({ isOpen: true }),
  close: () => set({ isOpen: false }),
  toggle: () => set((state) => ({ isOpen: !state.isOpen })),
}));

/** Opens the shortcuts help from outside React. */
export function openShortcutsHelp(): void {
  useShortcutsHelpStore.getState().open();
}

/** Closes the shortcuts help from outside React. */
export function closeShortcutsHelp(): void {
  useShortcutsHelpStore.getState().close();
}

/** Toggles the shortcuts help from outside React. */
export function toggleShortcutsHelp(): void {
  useShortcutsHelpStore.getState().toggle();
}