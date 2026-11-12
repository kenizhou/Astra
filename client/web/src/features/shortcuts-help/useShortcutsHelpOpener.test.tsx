// SPDX-License-Identifier: GPL-3.0-only
// The bare-character opener beside the palette's: ? opens the shortcuts help
// and toggles it shut, a claimed key is left to its handler, held repeats
// never re-toggle, and the editable-target guard keeps ? a plain character
// while an input, textarea, or contenteditable holds the focus (the palette
// input included). Ctrl+K stays the island's and Ctrl+Shift+P stays the
// palette's.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { AmbientIsland } from '@/components/layout/AmbientIsland';
import { CommandPalette } from '@/features/command-palette/CommandPalette';
import { useCommandPaletteStore } from '@/features/command-palette/useCommandPalette';
import { useCommandPaletteOpener } from '@/features/command-palette/useCommandPaletteOpener';
import { useAgentStore } from '@/features/agent/agentStore';
import { useListStore } from '@/stores/listStore';
import { useUiStore } from '@/stores/uiStore';

import { ShortcutsHelp } from './ShortcutsHelp';
import { useShortcutsHelpStore } from './useShortcutsHelp';
import { useShortcutsHelpOpener } from './useShortcutsHelpOpener';

const HELP = { name: 'Keyboard shortcuts' } as const;
const PALETTE = { name: 'Command palette' } as const;

/** Both openers and overlays mounted, as the shell holds them. */
function Harness() {
  useShortcutsHelpOpener();
  useCommandPaletteOpener();
  return (
    <>
      <AmbientIsland />
      <CommandPalette />
      <ShortcutsHelp />
    </>
  );
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useUiStore.setState({ agentOpen: false });
  useListStore.setState({ search: null });
  useAgentStore.getState().reset();
  useCommandPaletteStore.setState({ isOpen: false });
  useShortcutsHelpStore.setState({ isOpen: false });
});

afterEach(() => {
  useCommandPaletteStore.setState({ isOpen: false });
  useShortcutsHelpStore.setState({ isOpen: false });
  cleanup();
});

describe('the shortcuts help keyboard opener', () => {
  it('? opens the help and a second ? toggles it shut', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: '?' });
    expect(screen.getByRole('dialog', HELP)).toBeTruthy();
    fireEvent.keyDown(document, { key: '?' });
    expect(screen.queryByRole('dialog', HELP)).toBeNull();
  });

  it('? with shift held still opens: the logical key already carries it', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: '?', shiftKey: true });
    expect(screen.getByRole('dialog', HELP)).toBeTruthy();
  });

  it('leaves a ? another handler already claimed', () => {
    render(<Harness />);
    const event = new KeyboardEvent('keydown', { key: '?', cancelable: true });
    event.preventDefault();
    document.dispatchEvent(event);
    expect(screen.queryByRole('dialog', HELP)).toBeNull();
  });

  it('a held ? never re-toggles', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: '?' });
    expect(screen.getByRole('dialog', HELP)).toBeTruthy();
    fireEvent.keyDown(document, { key: '?', repeat: true });
    expect(screen.getByRole('dialog', HELP)).toBeTruthy();
  });

  it('ctrl, meta, or alt beside ? keeps the key with its own binding', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: '?', ctrlKey: true });
    fireEvent.keyDown(document, { key: '?', metaKey: true });
    fireEvent.keyDown(document, { key: '?', altKey: true });
    expect(screen.queryByRole('dialog', HELP)).toBeNull();
  });

  it('? stays a plain character while an input holds the focus', () => {
    render(<Harness />);
    const field = document.createElement('input');
    document.body.appendChild(field);
    field.focus();
    fireEvent.keyDown(field, { key: '?' });
    expect(screen.queryByRole('dialog', HELP)).toBeNull();
    field.remove();
  });

  it('? stays a plain character while a textarea holds the focus', () => {
    render(<Harness />);
    const field = document.createElement('textarea');
    document.body.appendChild(field);
    field.focus();
    fireEvent.keyDown(field, { key: '?' });
    expect(screen.queryByRole('dialog', HELP)).toBeNull();
    field.remove();
  });

  it('? stays a plain character while a contenteditable holds the focus', () => {
    render(<Harness />);
    const editor = document.createElement('div');
    editor.setAttribute('contenteditable', 'true');
    document.body.appendChild(editor);
    editor.focus();
    fireEvent.keyDown(editor, { key: '?' });
    expect(screen.queryByRole('dialog', HELP)).toBeNull();
    editor.remove();
  });

  it('? stays a plain character while the palette input holds the focus', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: 'P', ctrlKey: true, shiftKey: true });
    const field = screen.getByRole('textbox', { name: 'Search commands' });
    fireEvent.keyDown(field, { key: '?' });
    expect(screen.queryByRole('dialog', HELP)).toBeNull();
    expect(screen.getByRole('dialog', PALETTE)).toBeTruthy();
  });

  it('Ctrl+K still opens the island and never the help', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: 'k', ctrlKey: true });
    expect(screen.getByLabelText('Search scope')).toBeTruthy();
    expect(screen.queryByRole('dialog', HELP)).toBeNull();
  });

  it('Ctrl+Shift+P still opens the palette and never the help', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: 'P', ctrlKey: true, shiftKey: true });
    expect(screen.getByRole('dialog', PALETTE)).toBeTruthy();
    expect(screen.queryByRole('dialog', HELP)).toBeNull();
  });
});