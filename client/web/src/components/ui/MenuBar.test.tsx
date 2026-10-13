// SPDX-License-Identifier: GPL-3.0-only
// The application menus: each one opens from its button and only one stands
// at a time, Escape and a click outside close the standing one, and every
// entry acts: New Mail opens the composer the New message action's way (and
// stands disabled without an account), Add account and Preferences open
// their surfaces, Sync now dispatches the refresh intent, Close Window
// closes the Tauri window, the View radios dispatch the view-mode intent
// against the list store, Toggle Fullscreen flips the window's real state,
// the Tools radios dispatch the theme intent against the shell's mode, and
// About opens the settings screen on its About category.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const windowApi = vi.hoisted(() => ({
  close: vi.fn(async () => undefined),
  isFullscreen: vi.fn(async () => false),
  setFullscreen: vi.fn(async () => undefined),
}));

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: windowApi.close,
    isFullscreen: windowApi.isFullscreen,
    setFullscreen: windowApi.setFullscreen,
  }),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { MenuBar } from './MenuBar';
import { useComposerStore } from '@/features/composer/composerStore';
import { useSettingsStore } from '@/features/settings/settingsStore';
import { useAccountStore } from '@/stores/accountStore';
import { useListStore } from '@/stores/listStore';
import { useUiStore } from '@/stores/uiStore';

beforeEach(() => {
  windowApi.close.mockClear();
  windowApi.isFullscreen.mockClear();
  windowApi.isFullscreen.mockResolvedValue(false);
  windowApi.setFullscreen.mockClear();
  ipc.dispatch.mockReset();
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useListStore.setState({ viewMode: 'flat' });
  useUiStore.setState({
    themeMode: 'follow_host',
    accountSetupOpen: false,
    settingsOpen: false,
  });
  useSettingsStore.getState().requestCategory(null);
  useComposerStore.getState().closePane();
});

afterEach(() => {
  cleanup();
});

/** Opens one menu by its button. */
function openMenu(label: string): void {
  fireEvent.click(screen.getByRole('button', { name: label }));
}

describe('the menu bar chrome', () => {
  it('opens each menu from its button and keeps only one open', () => {
    render(<MenuBar />);

    openMenu('File');
    expect(screen.getByRole('menu', { name: 'File' })).toBeTruthy();

    openMenu('View');
    expect(screen.queryByRole('menu', { name: 'File' })).toBeNull();
    expect(screen.getByRole('menu', { name: 'View' })).toBeTruthy();

    openMenu('Tools');
    expect(screen.getByRole('menu', { name: 'Tools' })).toBeTruthy();
    openMenu('Help');
    expect(screen.queryByRole('menu', { name: 'Tools' })).toBeNull();
    expect(screen.getByRole('menu', { name: 'Help' })).toBeTruthy();
  });

  it('closes the standing menu on Escape', () => {
    render(<MenuBar />);
    openMenu('File');

    fireEvent.keyDown(document, { key: 'Escape' });

    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('closes the standing menu on a click outside', () => {
    render(<MenuBar />);
    openMenu('File');

    fireEvent.mouseDown(document.body);

    expect(screen.queryByRole('menu')).toBeNull();
  });
});

describe('the File menu', () => {
  it('opens the composer from New Mail, the New message action path', () => {
    render(<MenuBar />);
    openMenu('File');

    fireEvent.click(screen.getByRole('menuitem', { name: 'New Mail' }));

    expect(useComposerStore.getState().open).toBe(true);
    expect(useComposerStore.getState().from).toBe('work');
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('stands New Mail disabled while no account is configured', () => {
    useAccountStore.setState({ accounts: [] });
    render(<MenuBar />);
    openMenu('File');

    const item = screen.getByRole('menuitem', { name: 'New Mail' });
    expect(item.getAttribute('aria-disabled')).toBe('true');
    fireEvent.click(item);

    expect(useComposerStore.getState().open).toBe(false);
  });

  it('opens the account setup from Add account', () => {
    render(<MenuBar />);
    openMenu('File');

    fireEvent.click(screen.getByRole('menuitem', { name: 'Add account' }));

    expect(useUiStore.getState().accountSetupOpen).toBe(true);
  });

  it('dispatches the refresh intent from Sync now', () => {
    render(<MenuBar />);
    openMenu('File');

    fireEvent.click(screen.getByRole('menuitem', { name: 'Sync now' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith('refresh_mail');
  });

  it('opens the settings screen from Preferences', () => {
    render(<MenuBar />);
    openMenu('File');

    fireEvent.click(screen.getByRole('menuitem', { name: 'Preferences' }));

    expect(useUiStore.getState().settingsOpen).toBe(true);
    expect(useSettingsStore.getState().requestedCategory).toBeNull();
  });

  it('closes the window from Close Window', async () => {
    render(<MenuBar />);
    openMenu('File');

    fireEvent.click(screen.getByRole('menuitem', { name: 'Close Window' }));

    await waitFor(() => expect(windowApi.close).toHaveBeenCalledTimes(1));
  });
});

describe('the View menu', () => {
  it('marks the standing view mode and dispatches the other', () => {
    render(<MenuBar />);
    openMenu('View');

    expect(screen.getByRole('menuitemradio', { name: 'Flat' }).getAttribute('aria-checked')).toBe(
      'true',
    );
    expect(
      screen.getByRole('menuitemradio', { name: 'Threaded' }).getAttribute('aria-checked'),
    ).toBe('false');

    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Threaded' }));

    expect(ipc.dispatch).toHaveBeenCalledWith({ set_view_mode: 'threaded' });
  });

  it('follows the list store when it stands threaded', () => {
    useListStore.setState({ viewMode: 'threaded' });
    render(<MenuBar />);
    openMenu('View');

    expect(
      screen.getByRole('menuitemradio', { name: 'Threaded' }).getAttribute('aria-checked'),
    ).toBe('true');
  });

  it('toggles the window fullscreen from Toggle Fullscreen', async () => {
    render(<MenuBar />);
    openMenu('View');

    fireEvent.click(screen.getByRole('menuitem', { name: 'Toggle Fullscreen' }));

    await waitFor(() => expect(windowApi.setFullscreen).toHaveBeenCalledWith(true));
  });
});

describe('the Tools menu', () => {
  it('marks the standing theme mode and dispatches another', () => {
    render(<MenuBar />);
    openMenu('Tools');

    expect(
      screen.getByRole('menuitemradio', { name: 'Follow host' }).getAttribute('aria-checked'),
    ).toBe('true');

    fireEvent.click(screen.getByRole('menuitemradio', { name: 'Dark' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_theme":{"mode":"dark"}}}',
    );
    // The override lands only through the republished block, never locally.
    expect(useUiStore.getState().themeMode).toBe('follow_host');
  });

  it('follows the shell mode when it stands dark', () => {
    useUiStore.setState({ themeMode: 'dark' });
    render(<MenuBar />);
    openMenu('Tools');

    expect(screen.getByRole('menuitemradio', { name: 'Dark' }).getAttribute('aria-checked')).toBe(
      'true',
    );
  });
});

describe('the Help menu', () => {
  it('opens the settings screen on the About category', () => {
    render(<MenuBar />);
    openMenu('Help');

    fireEvent.click(screen.getByRole('menuitem', { name: 'About Astra Mail' }));

    expect(useUiStore.getState().settingsOpen).toBe(true);
    expect(useSettingsStore.getState().requestedCategory).toBe('about');
  });
});
