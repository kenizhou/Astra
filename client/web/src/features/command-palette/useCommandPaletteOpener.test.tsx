// SPDX-License-Identifier: GPL-3.0-only
// The keyboard opener beside the search island: Ctrl+Shift+P (or Cmd+Shift+P)
// opens the palette and toggles it shut, a claimed key is left to its handler,
// and Ctrl+K still belongs to the island alone. The Search mail command opens
// the island through the seam end to end.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { AmbientIsland } from '@/components/layout/AmbientIsland';
import { useAgentStore } from '@/features/agent/agentStore';
import { useListStore } from '@/stores/listStore';
import { useUiStore } from '@/stores/uiStore';

import { CommandPalette } from './CommandPalette';
import { useCommandPaletteStore } from './useCommandPalette';
import { useCommandPaletteOpener } from './useCommandPaletteOpener';

const DIALOG = { name: 'Command palette' } as const;

/** The opener mounted beside the island and the overlay, as the shell holds them. */
function Harness() {
  useCommandPaletteOpener();
  return (
    <>
      <AmbientIsland />
      <CommandPalette />
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
});

afterEach(() => {
  useCommandPaletteStore.setState({ isOpen: false });
  cleanup();
});

describe('the palette keyboard opener', () => {
  it('Ctrl+Shift+P opens the palette and leaves the island folded', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: 'P', ctrlKey: true, shiftKey: true });
    expect(screen.getByRole('dialog', DIALOG)).toBeTruthy();
    expect(screen.queryByLabelText('Search scope')).toBeNull();
  });

  it('Cmd+Shift+P opens the palette on the mac modifier', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: 'P', metaKey: true, shiftKey: true });
    expect(screen.getByRole('dialog', DIALOG)).toBeTruthy();
  });

  it('a second Ctrl+Shift+P toggles the palette shut', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: 'P', ctrlKey: true, shiftKey: true });
    expect(screen.getByRole('dialog', DIALOG)).toBeTruthy();
    fireEvent.keyDown(document, { key: 'P', ctrlKey: true, shiftKey: true });
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });

  it('leaves a Ctrl+Shift+P another handler already claimed', () => {
    render(<Harness />);
    const event = new KeyboardEvent('keydown', {
      key: 'P',
      ctrlKey: true,
      shiftKey: true,
      cancelable: true,
    });
    event.preventDefault();
    document.dispatchEvent(event);
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });

  it('Ctrl+K still opens the island and never the palette', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: 'k', ctrlKey: true });
    expect(screen.getByLabelText('Search scope')).toBeTruthy();
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });

  it('the Search mail command opens the island through the seam', () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: 'P', ctrlKey: true, shiftKey: true });
    const field = screen.getByRole('textbox', { name: 'Search commands' });
    fireEvent.change(field, { target: { value: 'search mail' } });
    fireEvent.keyDown(field, { key: 'Enter' });
    expect(screen.getByLabelText('Search scope')).toBeTruthy();
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });
});
