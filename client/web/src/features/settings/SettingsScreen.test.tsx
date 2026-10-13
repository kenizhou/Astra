// SPDX-License-Identifier: GPL-3.0-only
// The settings screen: the modal over the shell with the category source
// list beside the detail panel, the desktop pattern; the categories render
// exactly what ships, Escape and the Close return to the shell wherever
// the focus stands (the gear that opened it included), the dialog is
// caged (the focus moves in on open, the modal names itself), the detail
// switches with the source list, and the surface pull hydrates on open.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), settings: vi.fn(), mcpStatus: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  settings: ipc.settings,
  mcpStatus: ipc.mcpStatus,
}));

import { SettingsScreen } from './SettingsScreen';
import { useSettingsStore } from './settingsStore';
import { useUiStore } from '@/stores/uiStore';
import type { SettingsSnapshot } from '@/services/ipc';

function snapshot(over: Partial<SettingsSnapshot> = {}): SettingsSnapshot {
  return {
    settings: {
      theme: 'follow_host',
      twenty_four_clock: false,
      zone: 'Etc/UTC',
      first_day_of_week: 'mo',
      horizon_days: 42,
      quote_below: true,
      view_mode: 'flat',
    },
    accounts: [],
    about: { version: '0.1.0' },
    ...over,
  };
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.settings.mockReset();
  ipc.settings.mockResolvedValue(snapshot());
  ipc.mcpStatus.mockReset();
  ipc.mcpStatus.mockResolvedValue({
    status: 'off',
    endpoint: '\\\\.\\pipe\\com.kenizhou.astra.mcp',
    allowed: [],
    relay_command: [],
  });
  useUiStore.getState().setSettingsOpen(true);
});

afterEach(() => {
  useUiStore.getState().setSettingsOpen(false);
  cleanup();
});

describe('the settings screen', () => {
  it('renders nothing while the shell holds it closed', () => {
    useUiStore.getState().setSettingsOpen(false);
    render(<SettingsScreen />);
    expect(screen.queryByRole('dialog', { name: 'Settings' })).toBeNull();
  });

  it('renders the seven categories that ship and no others', () => {
    render(<SettingsScreen />);
    const list = screen.getByLabelText('Settings categories');
    const labels = Array.from(list.querySelectorAll('button')).map((b) => b.textContent);
    expect(labels).toEqual([
      'General',
      'Calendar',
      'Reading',
      'Composing',
      'Accounts',
      'Advanced',
      'About',
    ]);
  });

  it('switches the detail panel with the source list', () => {
    render(<SettingsScreen />);
    const detail = screen.getByLabelText('Settings detail');

    fireEvent.click(screen.getByRole('button', { name: 'About' }));
    expect(detail.textContent).toContain('0.1.0');

    fireEvent.click(screen.getByRole('button', { name: 'General' }));
    expect(screen.getByLabelText('Theme')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Advanced' }));
    expect(screen.getByLabelText('AI assistant access')).toBeTruthy();
  });

  it('returns to the shell from Close and from Escape', () => {
    render(<SettingsScreen />);

    fireEvent.click(screen.getByRole('button', { name: 'Close settings' }));
    expect(useUiStore.getState().settingsOpen).toBe(false);

    act(() => {
      useUiStore.getState().setSettingsOpen(true);
    });
    fireEvent.keyDown(screen.getByRole('dialog', { name: 'Settings' }), { key: 'Escape' });
    expect(useUiStore.getState().settingsOpen).toBe(false);
  });

  it('closes on Escape when the focus never entered the dialog', () => {
    // The gear opens the screen and keeps the focus itself: the Escape a
    // moment later stands outside the dialog and must still close it.
    const gear = document.createElement('button');
    gear.type = 'button';
    gear.textContent = 'Settings';
    document.body.appendChild(gear);
    gear.focus();
    render(<SettingsScreen />);

    fireEvent.keyDown(gear, { key: 'Escape' });

    expect(useUiStore.getState().settingsOpen).toBe(false);
    gear.remove();
  });

  it('moves the focus into the dialog on open and names itself a modal', () => {
    render(<SettingsScreen />);
    const dialog = screen.getByRole('dialog', { name: 'Settings' });

    expect(document.activeElement).toBe(dialog);
    expect(dialog.getAttribute('aria-modal')).toBe('true');
  });

  it('hydrates from the surface pull when it opens', async () => {
    render(<SettingsScreen />);

    await waitFor(() => expect(ipc.settings).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(useSettingsStore.getState().about.version).toBe('0.1.0'));
  });

  it('opens on the category a caller asked for and spends the ask', async () => {
    useSettingsStore.getState().requestCategory('about');
    render(<SettingsScreen />);

    await waitFor(() =>
      expect(screen.getByLabelText('Settings detail').textContent).toContain('0.1.0'),
    );
    expect(useSettingsStore.getState().requestedCategory).toBeNull();
  });

  it('keeps rendering when the pull refuses', async () => {
    ipc.settings.mockRejectedValue(new Error('the pull refused'));
    const errorLog = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    render(<SettingsScreen />);

    await waitFor(() => expect(ipc.settings).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('dialog', { name: 'Settings' })).toBeTruthy();
    errorLog.mockRestore();
  });
});
