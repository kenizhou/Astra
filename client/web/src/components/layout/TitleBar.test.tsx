// SPDX-License-Identifier: GPL-3.0-only
// The double-click maximise is scoped to the drag regions: a double click on
// the buttons or the island rides the bubble to the header, so the handler
// lives on the regions alone and a control can never net an extra toggle.
// The island itself: a click or Ctrl+K opens it into the search box, typing
// debounces the query across, Escape clears a typed query at once and folds
// an empty one, Enter submits without the pause and keeps the query in the
// session's history, a history pick runs it again, and the scope dropdown
// narrows the live search. The trailing dot is the agent's presence and
// opens the agent panel. The settings entry opens the modal, and the theme
// button cycles the override through the intent, so the persisted block
// stays the one truth.
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
import { useAgentStore } from '@/features/agent/agentStore';
import { useListStore } from '@/stores/listStore';
import { useSearchHistoryStore } from '@/stores/searchHistoryStore';
import { useUiStore } from '@/stores/uiStore';
import type { SearchState } from '@/services/surfaces';

/** Opens the island so its search field and scope dropdown stand. */
function expandIsland(): void {
  const pill = screen.queryByRole('button', { name: 'Search mail' });
  if (pill !== null) {
    fireEvent.click(pill);
  }
}

beforeEach(() => {
  windowApi.toggleMaximize.mockClear();
  ipc.dispatch.mockReset();
  useUiStore.setState({ themeMode: 'light', agentOpen: false });
  useListStore.setState({ search: null });
  useAgentStore.getState().reset();
  useSearchHistoryStore.getState().clear();
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
    fireEvent.doubleClick(screen.getByRole('button', { name: 'Open agent panel' }));
    fireEvent.doubleClick(screen.getByRole('button', { name: 'File' }));
    fireEvent.doubleClick(screen.getByLabelText('Search mail'));

    expandIsland();
    fireEvent.doubleClick(screen.getByLabelText('Search mail'));
    fireEvent.doubleClick(screen.getByLabelText('Search scope'));

    // The dblclick events bubble to the header; nothing there may answer.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(windowApi.toggleMaximize).not.toHaveBeenCalled();
  });

  it('keeps toggling maximise from the drag regions beside the island', async () => {
    const { container } = render(<TitleBar />);

    // The island sits between two drag regions; both still answer for it.
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

    // The follow-host state reads as Auto; the title names the next step.
    const button = screen.getByRole('button', { name: 'Toggle theme' });
    expect(button.textContent).toBe('Auto');
    expect(button.title).toBe('Following the host; switch to light');

    fireEvent.click(screen.getByRole('button', { name: 'Toggle theme' }));
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_theme":{"mode":"light"}}}',
    );
  });
});

describe('the ambient island', () => {
  it('opens on Ctrl+K and hands the field the focus', () => {
    render(<TitleBar />);

    fireEvent.keyDown(document, { key: 'k', ctrlKey: true });

    expect(document.activeElement).toBe(screen.getByLabelText('Search mail'));
  });

  it('ignores a Ctrl+K another handler already claimed', () => {
    render(<TitleBar />);

    const event = new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, cancelable: true });
    event.preventDefault();
    document.dispatchEvent(event);

    expect(screen.queryByLabelText('Search scope')).toBeNull();
  });

  it('folds back to the pill on Escape over an empty field', () => {
    render(<TitleBar />);
    expandIsland();

    fireEvent.keyDown(screen.getByLabelText('Search mail'), { key: 'Escape' });

    expect(screen.queryByLabelText('Search scope')).toBeNull();
    expect(screen.getByRole('button', { name: 'Search mail' })).toBeTruthy();
  });

  it('keeps a submitted query in the session history and runs a pick again', () => {
    render(<TitleBar />);
    expandIsland();

    fireEvent.change(screen.getByLabelText('Search mail'), { target: { value: 'from:ada' } });
    fireEvent.keyDown(screen.getByLabelText('Search mail'), { key: 'Enter' });

    expect(ipc.dispatch).toHaveBeenCalledWith({ search: { query: 'from:ada' } });
    expect(useSearchHistoryStore.getState().queries).toEqual(['from:ada']);

    // Clearing the field drops the history down; a pick re-runs the search.
    fireEvent.keyDown(screen.getByLabelText('Search mail'), { key: 'Escape' });
    ipc.dispatch.mockClear();
    fireEvent.click(screen.getByRole('option', { name: 'from:ada' }));

    expect(ipc.dispatch).toHaveBeenCalledWith({ search: { query: 'from:ada' } });
    expect((screen.getByLabelText('Search mail') as HTMLInputElement).value).toBe('from:ada');
  });

  it('shows no history dropdown until a query has been submitted', () => {
    render(<TitleBar />);
    expandIsland();

    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('maps the agent state to the presence dot in rising attention', () => {
    render(<TitleBar />);
    expect(screen.getByRole('button', { name: 'Open agent panel' })).toBeTruthy();

    act(() => {
      useAgentStore.setState({ turn: 'turn-1' });
    });
    expect(screen.getByRole('button', { name: 'Agent working; open agent panel' })).toBeTruthy();

    act(() => {
      useAgentStore.getState().addApproval({
        id: 'ap-1',
        tool_call_id: 'tc-1',
        tool: 'mail.send',
        level: 'send',
        args: {},
      });
    });
    expect(screen.getByRole('button', { name: 'Approval pending; open agent panel' })).toBeTruthy();
  });

  it('opens the agent panel from the dot without opening the search', () => {
    render(<TitleBar />);

    fireEvent.click(screen.getByRole('button', { name: 'Open agent panel' }));

    expect(useUiStore.getState().agentOpen).toBe(true);
    expect(screen.queryByLabelText('Search scope')).toBeNull();
  });
});

describe('the island search box', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('dispatches the typed query once 500 ms of quiet have passed', () => {
    render(<TitleBar />);
    expandIsland();

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
    expandIsland();

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
    expandIsland();

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
    expandIsland();

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
