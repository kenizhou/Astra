// SPDX-License-Identifier: GPL-3.0-only
// The pinned searches panel: the store gate, the saved list with its run,
// rename, and delete verbs, the empty state, the run that opens the island
// and dispatches the saved query alone (no scope rides it), and the
// Escape/outside dismissals with the bell trigger excepted. A mid-test store
// mutation under fake timers rides act().
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

const island = vi.hoisted(() => ({ open: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { PinnedSearchesPanel } from './PinnedSearchesPanel';
import { usePinnedSearchesStore, type PinnedSearch } from './pinnedSearchesStore';
import { registerSearchIslandOpener } from '@/components/layout/searchIsland';

/** One saved search, overridable per test. */
function saved(id: string, over: Partial<PinnedSearch> = {}): PinnedSearch {
  return { id, name: `Search ${id}`, query: 'from:' + id + '@example.test', ...over };
}

/** Seeds the saved searches and opens the panel, as the bell does. */
function open(searches: PinnedSearch[] = []): void {
  usePinnedSearchesStore.setState({ searches });
  usePinnedSearchesStore.getState().openPanel();
}

beforeEach(() => {
  vi.useFakeTimers();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  island.open.mockReset();
  localStorage.clear();
  usePinnedSearchesStore.setState({ searches: [], panelOpen: false });
  registerSearchIslandOpener(island.open);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  registerSearchIslandOpener(null);
  localStorage.clear();
});

describe('the panel gate', () => {
  it('renders nothing while the gate is closed', () => {
    render(<PinnedSearchesPanel />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('lists the saved searches with their names and queries', () => {
    open([saved('a', { name: 'Ada monitors', query: 'from:ada monitor' }), saved('b')]);
    render(<PinnedSearchesPanel />);

    const dialog = screen.getByRole('dialog', { name: 'Saved searches' });
    expect(within(dialog).getByText('Ada monitors')).toBeTruthy();
    expect(within(dialog).getByText('from:ada monitor')).toBeTruthy();
    expect(within(dialog).getByText('Search b')).toBeTruthy();
  });

  it('names the empty state while nothing is saved', () => {
    open();
    render(<PinnedSearchesPanel />);
    expect(screen.getByText('No saved searches yet.')).toBeTruthy();
  });
});

describe('the run verb', () => {
  it('opens the island and dispatches the saved query alone, then closes', () => {
    open([saved('a', { name: 'Ada monitors', query: 'from:ada monitor' })]);
    render(<PinnedSearchesPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Run Ada monitors' }));

    expect(island.open).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({ search: { query: 'from:ada monitor' } });
    expect(usePinnedSearchesStore.getState().panelOpen).toBe(false);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('dispatches no scope intent: the query carries no narrowing', () => {
    open([saved('a', { query: 'monitor' })]);
    render(<PinnedSearchesPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Run Search a' }));

    for (const call of ipc.dispatch.mock.calls) {
      expect(call[0]).not.toHaveProperty('set_search_scope');
    }
  });
});

describe('the rename and delete verbs', () => {
  it('renames a search in place, prefilling the form and keeping the query', () => {
    open([saved('a', { name: 'Original', query: 'from:ada' })]);
    render(<PinnedSearchesPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Rename Original' }));
    const name = screen.getByRole('textbox', { name: 'Name' }) as HTMLInputElement;
    expect(name.value).toBe('Original');

    fireEvent.change(name, { target: { value: 'Renamed' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(usePinnedSearchesStore.getState().searches).toEqual([
      { id: 'a', name: 'Renamed', query: 'from:ada' },
    ]);
    expect(screen.queryByRole('textbox', { name: 'Name' })).toBeNull();
    expect(screen.getByText('Renamed')).toBeTruthy();
  });

  it('keeps Save disabled while the rename is blank', () => {
    open([saved('a', { name: 'Original' })]);
    render(<PinnedSearchesPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Rename Original' }));
    const name = screen.getByRole('textbox', { name: 'Name' });
    fireEvent.change(name, { target: { value: '   ' } });
    expect((screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.change(name, { target: { value: 'Named' } });
    expect((screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('Cancel closes the rename form without saving', () => {
    open([saved('a', { name: 'Original' })]);
    render(<PinnedSearchesPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Rename Original' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Name' }), {
      target: { value: 'Never saved' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(usePinnedSearchesStore.getState().searches[0]?.name).toBe('Original');
    expect(screen.queryByRole('textbox', { name: 'Name' })).toBeNull();
  });

  it('deletes a search from the list and the store', () => {
    open([saved('a'), saved('b')]);
    render(<PinnedSearchesPanel />);

    fireEvent.click(screen.getByRole('button', { name: 'Delete Search a' }));

    expect(usePinnedSearchesStore.getState().searches.map((s) => s.id)).toEqual(['b']);
    expect(screen.queryByText('Search a')).toBeNull();
    expect(screen.getByText('Search b')).toBeTruthy();
  });
});

describe('the dismissals', () => {
  it('Escape closes the panel through the gate', () => {
    open([saved('a')]);
    render(<PinnedSearchesPanel />);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(usePinnedSearchesStore.getState().panelOpen).toBe(false);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('a pointer down outside closes the panel', () => {
    open();
    render(<PinnedSearchesPanel />);

    fireEvent.mouseDown(document.body);
    expect(usePinnedSearchesStore.getState().panelOpen).toBe(false);
  });

  it('the bell trigger is excepted, so its own toggle is not doubled', () => {
    open();
    render(
      <>
        <button type="button" data-pinned-searches-bell>
          Saved searches
        </button>
        <PinnedSearchesPanel />
      </>,
    );

    fireEvent.mouseDown(screen.getByRole('button', { name: 'Saved searches' }));
    expect(usePinnedSearchesStore.getState().panelOpen).toBe(true);
  });

  it('re-renders when the saved searches change while the panel stands', () => {
    open([saved('a')]);
    render(<PinnedSearchesPanel />);
    expect(screen.getByText('Search a')).toBeTruthy();

    act(() => {
      usePinnedSearchesStore.getState().save('Fresh', 'fresh query');
    });
    expect(screen.getByText('Fresh')).toBeTruthy();
  });
});
