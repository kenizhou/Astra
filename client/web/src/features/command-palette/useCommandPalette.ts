// SPDX-License-Identifier: GPL-3.0-only
// The palette's open state: a tiny standalone store the overlay reads, with
// module-scope verbs so non-React code (the keyboard opener, a future shortcut
// registry) can open, close, and toggle it without a hook.

import { create } from 'zustand';

interface CommandPaletteUiState {
  isOpen: boolean;
  open: () => void;
  close: () => void;
  toggle: () => void;
}

export const useCommandPaletteStore = create<CommandPaletteUiState>((set) => ({
  isOpen: false,
  open: () => set({ isOpen: true }),
  close: () => set({ isOpen: false }),
  toggle: () => set((state) => ({ isOpen: !state.isOpen })),
}));

/** Opens the palette from outside React. */
export function openCommandPalette(): void {
  useCommandPaletteStore.getState().open();
}

/** Closes the palette from outside React. */
export function closeCommandPalette(): void {
  useCommandPaletteStore.getState().close();
}

/** Toggles the palette from outside React. */
export function toggleCommandPalette(): void {
  useCommandPaletteStore.getState().toggle();
}
