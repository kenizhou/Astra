// SPDX-License-Identifier: GPL-3.0-only
// The shortcuts help overlay: closed renders null, the dialog names itself a
// modal, the rows reuse the settings inventory grouped by context in
// first-appearance order, the overlay sits at the modal tier, the focus moves
// in on open and back on close, Escape and the backdrop close, and a click
// inside the dialog does not.
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { SHORTCUTS } from '@/features/settings/ShortcutsDetail';

import { ShortcutsHelp } from './ShortcutsHelp';
import { closeShortcutsHelp, openShortcutsHelp, useShortcutsHelpStore } from './useShortcutsHelp';

const DIALOG = { name: 'Keyboard shortcuts' } as const;

/** Mounts the overlay and opens it, the surface mounting fresh. */
function renderOpen(): void {
  render(<ShortcutsHelp />);
  act(() => {
    openShortcutsHelp();
  });
}

beforeEach(() => {
  useShortcutsHelpStore.setState({ isOpen: false });
});

afterEach(() => {
  useShortcutsHelpStore.setState({ isOpen: false });
  cleanup();
});

describe('the shortcuts help surface', () => {
  it('renders nothing while the store holds it closed', () => {
    render(<ShortcutsHelp />);
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });

  it('names itself a modal', () => {
    renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    expect(dialog.getAttribute('aria-modal')).toBe('true');
  });

  it('hints how to reopen and close', () => {
    renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    expect(dialog.textContent).toContain('Press ? to reopen. Esc closes.');
  });

  it('sits at the modal tier so it wins the tie above the palette', () => {
    renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    const overlay = dialog.parentElement!;
    expect(overlay.className).toContain('z-[var(--z-modal)]');
    expect(overlay.className).not.toContain('z-[var(--z-modal-backdrop)]');
  });

  it('groups the inventory rows by context in first-appearance order', () => {
    renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);

    const contexts = [...new Set(SHORTCUTS.map((entry) => entry.context))];
    const groups = within(dialog).getAllByRole('region');
    expect(groups.map((group) => group.getAttribute('aria-label'))).toEqual(contexts);

    for (const context of contexts) {
      const group = within(dialog).getByRole('region', { name: context });
      const rows = within(group).getAllByRole('listitem');
      const entries = SHORTCUTS.filter((entry) => entry.context === context);
      expect(rows).toHaveLength(entries.length);
      rows.forEach((row, index) => {
        expect(row.textContent).toContain(entries[index]!.action);
        expect(row.textContent).toContain(entries[index]!.keys);
      });
    }
  });

  it('lists the ? opener beside the palette row under Everywhere', () => {
    renderOpen();
    const group = screen.getByRole('region', { name: 'Everywhere' });
    const rows = within(group).getAllByRole('listitem');
    expect(rows.map((row) => row.textContent)).toEqual([
      'Open the search islandCtrl+K',
      'Open the command paletteCtrl+Shift+P',
      'Open the shortcuts help?',
    ]);
  });

  it('moves the focus into the dialog on open and back on close', () => {
    const opener = document.createElement('button');
    opener.type = 'button';
    document.body.appendChild(opener);
    opener.focus();

    render(<ShortcutsHelp />);
    act(() => {
      openShortcutsHelp();
    });
    expect(document.activeElement).toBe(screen.getByRole('dialog', DIALOG));

    act(() => {
      closeShortcutsHelp();
    });
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  it('Escape closes the help', () => {
    renderOpen();
    fireEvent.keyDown(screen.getByRole('dialog', DIALOG), { key: 'Escape' });
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
    expect(useShortcutsHelpStore.getState().isOpen).toBe(false);
  });

  it('the backdrop closes but a click inside the dialog does not', () => {
    renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    fireEvent.mouseDown(dialog);
    expect(screen.getByRole('dialog', DIALOG)).toBeTruthy();

    fireEvent.mouseDown(dialog.parentElement!);
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });
});