// SPDX-License-Identifier: GPL-3.0-only
// The shortcuts help's open state: a tiny standalone store the overlay reads
// and the module-scope verbs move, so non-React code (the keyboard opener, a
// future shortcut registry) can open, close, and toggle it without a hook.
import { afterEach, describe, expect, it } from 'vitest';

import {
  closeShortcutsHelp,
  openShortcutsHelp,
  toggleShortcutsHelp,
  useShortcutsHelpStore,
} from './useShortcutsHelp';

afterEach(() => {
  useShortcutsHelpStore.setState({ isOpen: false });
});

describe('the shortcuts help store', () => {
  it('starts closed', () => {
    expect(useShortcutsHelpStore.getState().isOpen).toBe(false);
  });

  it('open and close move the flag', () => {
    useShortcutsHelpStore.getState().open();
    expect(useShortcutsHelpStore.getState().isOpen).toBe(true);
    useShortcutsHelpStore.getState().close();
    expect(useShortcutsHelpStore.getState().isOpen).toBe(false);
  });

  it('toggle flips the flag both ways', () => {
    useShortcutsHelpStore.getState().toggle();
    expect(useShortcutsHelpStore.getState().isOpen).toBe(true);
    useShortcutsHelpStore.getState().toggle();
    expect(useShortcutsHelpStore.getState().isOpen).toBe(false);
  });
});

describe('the module-scope shortcuts help verbs', () => {
  it('openShortcutsHelp opens from outside React', () => {
    openShortcutsHelp();
    expect(useShortcutsHelpStore.getState().isOpen).toBe(true);
  });

  it('closeShortcutsHelp closes from outside React', () => {
    openShortcutsHelp();
    closeShortcutsHelp();
    expect(useShortcutsHelpStore.getState().isOpen).toBe(false);
  });

  it('toggleShortcutsHelp flips from outside React', () => {
    toggleShortcutsHelp();
    expect(useShortcutsHelpStore.getState().isOpen).toBe(true);
    toggleShortcutsHelp();
    expect(useShortcutsHelpStore.getState().isOpen).toBe(false);
  });
});