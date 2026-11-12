// SPDX-License-Identifier: GPL-3.0-only
// The palette's open state: a tiny standalone store the overlay reads and
// the module-scope verbs move, so non-React code (the keyboard opener, a
// future shortcut registry) can open, close, and toggle it without a hook.
import { afterEach, describe, expect, it } from 'vitest';

import {
  closeCommandPalette,
  openCommandPalette,
  toggleCommandPalette,
  useCommandPaletteStore,
} from './useCommandPalette';

afterEach(() => {
  useCommandPaletteStore.setState({ isOpen: false });
});

describe('the command palette store', () => {
  it('starts closed', () => {
    expect(useCommandPaletteStore.getState().isOpen).toBe(false);
  });

  it('open and close move the flag', () => {
    useCommandPaletteStore.getState().open();
    expect(useCommandPaletteStore.getState().isOpen).toBe(true);
    useCommandPaletteStore.getState().close();
    expect(useCommandPaletteStore.getState().isOpen).toBe(false);
  });

  it('toggle flips the flag both ways', () => {
    useCommandPaletteStore.getState().toggle();
    expect(useCommandPaletteStore.getState().isOpen).toBe(true);
    useCommandPaletteStore.getState().toggle();
    expect(useCommandPaletteStore.getState().isOpen).toBe(false);
  });
});

describe('the module-scope palette verbs', () => {
  it('openCommandPalette opens from outside React', () => {
    openCommandPalette();
    expect(useCommandPaletteStore.getState().isOpen).toBe(true);
  });

  it('closeCommandPalette closes from outside React', () => {
    openCommandPalette();
    closeCommandPalette();
    expect(useCommandPaletteStore.getState().isOpen).toBe(false);
  });

  it('toggleCommandPalette flips from outside React', () => {
    toggleCommandPalette();
    expect(useCommandPaletteStore.getState().isOpen).toBe(true);
    toggleCommandPalette();
    expect(useCommandPaletteStore.getState().isOpen).toBe(false);
  });
});
