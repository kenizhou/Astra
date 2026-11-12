// SPDX-License-Identifier: GPL-3.0-only
// The palette overlay: the substring filter narrows the flat list, the arrows
// wrap, Enter runs the selected command and closes (the intent crossing the
// mocked ipc), Escape and the backdrop close, a disabled command shows its
// reason and never dispatches, the query and selection reset on every open,
// the empty state renders, the selection clamps against a shrinking list, and
// the focus moves in on open and back on close.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

const composer = vi.hoisted(() => ({ openNewMessageComposer: vi.fn() }));

vi.mock('@/features/composer/composeWindow', () => ({
  openNewMessageComposer: composer.openNewMessageComposer,
}));

import type { MessageRowView } from '@/services/surfaces';
import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { useUiStore } from '@/stores/uiStore';

import { CommandPalette } from './CommandPalette';
import { closeCommandPalette, openCommandPalette, useCommandPaletteStore } from './useCommandPalette';

const DIALOG = { name: 'Command palette' } as const;

/** One mailbox-list row, overridable per test. */
function row(over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key: { message: { account: 'work', key: 'imap:v1:u1' } },
    account_id: 'work',
    sender_name: 'Ada Lovelace',
    sender_address: 'ada@example.test',
    subject: 'Launch',
    sent_at: '2026-10-09T08:30:00Z',
    unread: true,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

/** Mounts the overlay and opens it, the surface mounting fresh. */
function renderOpen(): void {
  render(<CommandPalette />);
  act(() => {
    openCommandPalette();
  });
}

function input(): HTMLInputElement {
  return screen.getByRole('textbox', { name: 'Search commands' }) as HTMLInputElement;
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  composer.openNewMessageComposer.mockReset();
  composer.openNewMessageComposer.mockResolvedValue(undefined);
  useUiStore.setState({
    shellView: 'mail',
    themeMode: 'follow_host',
    agentOpen: false,
    aiPanelOpen: false,
    settingsOpen: false,
    accountSetupOpen: false,
  });
  useAccountStore.setState({ accounts: [] });
  useFolderStore.setState({ accounts: [], selected: null, outboxAccount: null, unified: false, scope: null });
  useListStore.setState({ rows: [], selectedKey: null, selectedKeys: [] });
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useCommandPaletteStore.setState({ isOpen: false });
});

afterEach(() => {
  useCommandPaletteStore.setState({ isOpen: false });
  cleanup();
});

describe('the palette surface', () => {
  it('renders nothing while the store holds it closed', () => {
    render(<CommandPalette />);
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });

  it('names itself a modal and wires the listbox to the input', () => {
    renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    expect(dialog.getAttribute('aria-modal')).toBe('true');

    const field = input();
    expect(field.getAttribute('aria-controls')).toBe('command-palette-list');
    const list = screen.getByRole('listbox', { name: 'Commands' });
    expect(list.id).toBe('command-palette-list');

    const first = screen.getAllByRole('option')[0]!;
    expect(first.getAttribute('aria-selected')).toBe('true');
    expect(field.getAttribute('aria-activedescendant')).toBe(first.id);
  });

  it('sits at the modal tier so it opens above other modals', () => {
    renderOpen();
    const dialog = screen.getByRole('dialog', DIALOG);
    // The overlay carries the modal z-token, not the backdrop tier, and
    // mounts last in AppShell, so it wins the tie above Settings and setup.
    const overlay = dialog.parentElement!;
    expect(overlay.className).toContain('z-[var(--z-modal)]');
    expect(overlay.className).not.toContain('z-[var(--z-modal-backdrop)]');
  });

  it('moves the focus into the field on open and back on close', () => {
    const opener = document.createElement('button');
    opener.type = 'button';
    document.body.appendChild(opener);
    opener.focus();

    render(<CommandPalette />);
    act(() => {
      openCommandPalette();
    });
    expect(document.activeElement).toBe(input());

    act(() => {
      closeCommandPalette();
    });
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });
});

describe('the palette filter', () => {
  it('narrows the list by case-insensitive substring on the label', () => {
    renderOpen();
    fireEvent.change(input(), { target: { value: 'THEME' } });
    const options = screen.getAllByRole('option');
    expect(options).toHaveLength(1);
    expect(options[0]!.textContent).toContain('Cycle theme');
  });

  it('renders the empty state when nothing matches', () => {
    renderOpen();
    fireEvent.change(input(), { target: { value: 'zzzznope' } });
    expect(screen.getByTestId('command-palette-empty')).toBeTruthy();
    expect(screen.queryByRole('listbox')).toBeNull();
    expect(screen.queryAllByRole('option')).toHaveLength(0);
  });

  it('clamps the selection against a shrinking list', () => {
    renderOpen();
    const field = input();
    for (let step = 0; step < 5; step += 1) {
      fireEvent.keyDown(field, { key: 'ArrowDown' });
    }
    fireEvent.change(field, { target: { value: 'trash' } });
    expect(screen.getAllByRole('option')).toHaveLength(1);
    expect(field.getAttribute('aria-activedescendant')).toBe('command-option-go-trash');
  });
});

describe('the palette keyboard', () => {
  it('wraps ArrowUp to the last command and ArrowDown back to the first', () => {
    renderOpen();
    const field = input();
    expect(field.getAttribute('aria-activedescendant')).toBe('command-option-go-mail');

    fireEvent.keyDown(field, { key: 'ArrowUp' });
    expect(field.getAttribute('aria-activedescendant')).toBe('command-option-reading-off');

    fireEvent.keyDown(field, { key: 'ArrowDown' });
    expect(field.getAttribute('aria-activedescendant')).toBe('command-option-go-mail');
  });

  it('Enter runs the selected local command and closes', () => {
    renderOpen();
    fireEvent.change(input(), { target: { value: 'calendar' } });
    fireEvent.keyDown(input(), { key: 'Enter' });
    expect(useUiStore.getState().shellView).toBe('calendar');
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('Enter runs the selected intent through the ipc and closes', () => {
    renderOpen();
    fireEvent.change(input(), { target: { value: 'sync' } });
    fireEvent.keyDown(input(), { key: 'Enter' });
    expect(ipc.dispatch).toHaveBeenCalledWith('refresh_mail');
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
  });

  it('Enter on a mail action dispatches the intent for the selected row', () => {
    const selected = row();
    useListStore.setState({ rows: [selected], selectedKey: selected.key });
    renderOpen();
    fireEvent.change(input(), { target: { value: 'mark read' } });
    fireEvent.keyDown(input(), { key: 'Enter' });
    expect(ipc.dispatch).toHaveBeenCalledWith({ mark_read: { row: selected.key, read: true } });
  });

  it('Escape closes the palette', () => {
    renderOpen();
    fireEvent.keyDown(input(), { key: 'Escape' });
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
    expect(useCommandPaletteStore.getState().isOpen).toBe(false);
  });
});

describe('the palette pointer', () => {
  it('clicking an enabled command runs it and closes', () => {
    renderOpen();
    fireEvent.change(input(), { target: { value: 'calendar' } });
    fireEvent.click(screen.getByRole('option', { name: 'Go to Calendar' }));
    expect(useUiStore.getState().shellView).toBe('calendar');
    expect(screen.queryByRole('dialog', DIALOG)).toBeNull();
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

describe('the palette honest disable', () => {
  it('shows the reason and never dispatches for a disabled command', () => {
    renderOpen();
    fireEvent.change(input(), { target: { value: 'reply' } });
    expect(screen.getAllByText('Open a message first').length).toBeGreaterThan(0);

    const first = screen.getAllByRole('option')[0]!;
    expect(first.getAttribute('aria-disabled')).toBe('true');

    fireEvent.keyDown(input(), { key: 'Enter' });
    fireEvent.click(first);
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog', DIALOG)).toBeTruthy();
  });
});

describe('the palette reset on open', () => {
  it('clears the query and selection on every open', () => {
    renderOpen();
    fireEvent.change(input(), { target: { value: 'theme' } });
    fireEvent.keyDown(input(), { key: 'ArrowDown' });

    act(() => {
      closeCommandPalette();
    });
    act(() => {
      openCommandPalette();
    });

    const reopened = input();
    expect(reopened.value).toBe('');
    expect(reopened.getAttribute('aria-activedescendant')).toBe('command-option-go-mail');
  });
});
