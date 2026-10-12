// SPDX-License-Identifier: GPL-3.0-only
// The double-click maximise is scoped to the drag regions: a double click on
// the buttons or the search box rides the bubble to the header, so the
// handler lives on the regions alone and a control can never net an extra
// toggle. The search box itself: typing debounces the query across, Escape
// clears it at once, the scope dropdown narrows the live search, and the
// field echoes the query the snapshot's search state carries. The settings
// entry opens the modal, and the theme button cycles the override through
// the intent, so the persisted block stays the one truth.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const windowApi = vi.hoisted(() => ({
  toggleMaximize: vi.fn(async () => undefined),
  minimize: vi.fn(async () => undefined),
  close: vi.fn(async () => undefined),
}));

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    toggleMaximize: windowApi.toggleMaximize,
    minimize: windowApi.minimize,
    close: windowApi.close,
    isMaximized: async () => false,
    onResized: async () => () => undefined,
  }),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { TitleBar } from './TitleBar';
import { useListStore } from '@/stores/listStore';
import { useUiStore } from '@/stores/uiStore';
import type { SearchState } from '@/services/surfaces';

beforeEach(() => {
  windowApi.toggleMaximize.mockClear();
  ipc.dispatch.mockReset();
  useUiStore.setState({ themeMode: 'light' });
  useListStore.setState({ search: null });
});

afterEach(() => {
  cleanup();
});

describe('the title bar double-click maximise', () => {
  it('toggles maximise from a drag region', async () => {
    render(<TitleBar />);
    fireEvent.doubleClick(screen.getByText('Astra Mail'));
    await waitFor(() => expect(windowApi.toggleMaximize).toHaveBeenCalledTimes(1));
  });

  it('never toggles maximise from the buttons the header also holds', async () => {
    render(<TitleBar />);
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Add account' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Toggle theme' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Settings' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Minimise' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Maximise' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Close window' }));
    fireEvent.doubleClick(screen.getByLabelText('Search mail'));
    fireEvent.doubleClick(screen.getByLabelText('Search scope'));

    // The dblclick events bubble to the header; nothing there may answer.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(windowApi.toggleMaximize).not.toHaveBeenCalled();
  });

  it('keeps toggling maximise from the drag regions beside the search box', async () => {
    const { container } = render(<TitleBar />);

    // The box sits between two drag regions; both still answer for it.
    const regions = container.querySelectorAll('div[data-tauri-drag-region]');
    expect(regions).toHaveLength(2);
    fireEvent.doubleClick(regions[0] as HTMLElement);
    fireEvent.doubleClick(regions[1] as HTMLElement);
    await waitFor(() => expect(windowApi.toggleMaximize).toHaveBeenCalledTimes(2));
  });
});

describe('the title bar settings entry', () => {
  it('opens the settings modal from the gear', () => {
    useUiStore.getState().setSettingsOpen(false);
    render(<TitleBar />);

    fireEvent.click(screen.getByRole('button', { name: 'Settings' }));

    expect(useUiStore.getState().settingsOpen).toBe(true);
  });
});

describe('the title bar theme override cycle', () => {
  it('cycles light to dark through the settings intent', () => {
    useUiStore.setState({ themeMode: 'light' });
    render(<TitleBar />);

    fireEvent.click(screen.getByRole('button', { name: 'Toggle theme' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_theme":{"mode":"dark"}}}',
    );
    // The override lands only through the republished block, never locally.
    expect(useUiStore.getState().themeMode).toBe('light');
  });

  it('cycles dark back to following the host', () => {
    useUiStore.setState({ themeMode: 'dark' });
    render(<TitleBar />);

    fireEvent.click(screen.getByRole('button', { name: 'Toggle theme' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_theme":{"mode":"follow_host"}}}',
    );
  });

  it('cycles follow host to the light override and says where it stands', () => {
    useUiStore.setState({ themeMode: 'follow_host' });
    render(<TitleBar />);

    expect(screen.getByRole('button', { name: 'Toggle theme' }).textContent).toBe('Light');

    fireEvent.click(screen.getByRole('button', { name: 'Toggle theme' }));
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_theme":{"mode":"light"}}}',
    );
  });
});

describe('the title bar search box', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('dispatches the typed query once 500 ms of quiet have passed', () => {
    render(<TitleBar />);

    fireEvent.change(screen.getByLabelText('Search mail'), {
      target: { value: 'from:ada monitor' },
    });

    vi.advanceTimersByTime(499);
    expect(ipc.dispatch).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({ search: { query: 'from:ada monitor' } });
  });

  it('restarts the quiet window on every keystroke', () => {
    render(<TitleBar />);

    fireEvent.change(screen.getByLabelText('Search mail'), { target: { value: 'm' } });
    vi.advanceTimersByTime(300);
    fireEvent.change(screen.getByLabelText('Search mail'), { target: { value: 'mo' } });
    vi.advanceTimersByTime(300);
    expect(ipc.dispatch).not.toHaveBeenCalled();

    vi.advanceTimersByTime(200);
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({ search: { query: 'mo' } });
  });

  it('clears the search at once on Escape and cancels the pending query', () => {
    render(<TitleBar />);

    fireEvent.change(screen.getByLabelText('Search mail'), { target: { value: 'monitor' } });
    fireEvent.keyDown(screen.getByLabelText('Search mail'), { key: 'Escape' });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({ search: { query: null } });
    expect((screen.getByLabelText('Search mail') as HTMLInputElement).value).toBe('');

    vi.advanceTimersByTime(2000);
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
  });

  it('dispatches set_search_scope from the dropdown without a debounce', () => {
    render(<TitleBar />);

    fireEvent.change(screen.getByLabelText('Search scope'), {
      target: { value: 'current_folder' },
    });
    fireEvent.change(screen.getByLabelText('Search scope'), {
      target: { value: 'current_account' },
    });

    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      set_search_scope: { scope: 'current_folder' },
    });
    expect(ipc.dispatch).toHaveBeenCalledWith({
      set_search_scope: { scope: 'current_account' },
    });
  });

  it('echoes the live search state into the field', () => {
    const search: SearchState = {
      query: 'from:ada monitor',
      scope: 'current_folder',
      rows: [],
      depth_months: 3,
    };
    useListStore.setState({ search });

    render(<TitleBar />);

    expect((screen.getByLabelText('Search mail') as HTMLInputElement).value).toBe('from:ada monitor');
    expect((screen.getByLabelText('Search scope') as HTMLSelectElement).value).toBe('current_folder');
  });

  it('clears the field when the search exits elsewhere', () => {
    useListStore.setState({
      search: { query: 'monitor', scope: 'all_except_trash', rows: [], depth_months: 3 },
    });

    render(<TitleBar />);
    act(() => {
      useListStore.setState({ search: null });
    });

    expect((screen.getByLabelText('Search mail') as HTMLInputElement).value).toBe('');
  });
});
