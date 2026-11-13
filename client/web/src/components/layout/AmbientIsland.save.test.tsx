// SPDX-License-Identifier: GPL-3.0-only
// The island's Save search affordance: it stands only while a query is live,
// captures the live query under a name (the query alone, never the scope),
// prefills the name with the query, refuses a blank name, cancels without
// saving, folds away when the field blanks, and disables at the cap. The
// field's own typing behaviour stays pinned in TitleBar.test.tsx.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { AmbientIsland } from './AmbientIsland';
import {
  MAX_PINNED_SEARCHES,
  usePinnedSearchesStore,
} from '@/features/pinned-searches/pinnedSearchesStore';
import { useAgentStore } from '@/features/agent/agentStore';
import { useListStore } from '@/stores/listStore';
import { useSearchHistoryStore } from '@/stores/searchHistoryStore';

/** Opens the island so its field stands. */
function expand(): void {
  fireEvent.click(screen.getByRole('button', { name: 'Search mail' }));
}

/** Types into the field without submitting. */
function typeQuery(value: string): void {
  fireEvent.change(screen.getByLabelText('Search mail'), { target: { value } });
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  localStorage.clear();
  usePinnedSearchesStore.setState({ searches: [], panelOpen: false });
  useListStore.setState({ search: null });
  useAgentStore.getState().reset();
  useSearchHistoryStore.getState().clear();
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe('the Save search affordance', () => {
  it('stands absent while the island is folded and while the field is blank', () => {
    render(<AmbientIsland />);
    expect(screen.queryByRole('button', { name: 'Save search' })).toBeNull();

    expand();
    expect(screen.queryByRole('button', { name: 'Save search' })).toBeNull();

    typeQuery('   ');
    expect(screen.queryByRole('button', { name: 'Save search' })).toBeNull();
  });

  it('stands once a query is live, before any submit', () => {
    render(<AmbientIsland />);
    expand();

    typeQuery('from:ada monitor');

    expect(screen.getByRole('button', { name: 'Save search' })).toBeTruthy();
  });

  it('captures the live query under a name, the query alone', () => {
    render(<AmbientIsland />);
    expand();
    typeQuery('from:ada monitor');

    fireEvent.click(screen.getByRole('button', { name: 'Save search' }));
    const name = screen.getByRole('textbox', { name: 'Saved search name' }) as HTMLInputElement;
    expect(name.value).toBe('from:ada monitor');
    fireEvent.change(name, { target: { value: 'Ada monitors' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    const searches = usePinnedSearchesStore.getState().searches;
    expect(searches).toHaveLength(1);
    expect(searches[0]).toMatchObject({ name: 'Ada monitors', query: 'from:ada monitor' });
    expect(Object.keys(searches[0] ?? {}).sort()).toEqual(['id', 'name', 'query']);
    expect(screen.queryByRole('textbox', { name: 'Saved search name' })).toBeNull();
  });

  it('keeps the query as the default name', () => {
    render(<AmbientIsland />);
    expand();
    typeQuery('monitor');

    fireEvent.click(screen.getByRole('button', { name: 'Save search' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(usePinnedSearchesStore.getState().searches[0]).toMatchObject({
      name: 'monitor',
      query: 'monitor',
    });
  });

  it('refuses a blank name', () => {
    render(<AmbientIsland />);
    expand();
    typeQuery('monitor');

    fireEvent.click(screen.getByRole('button', { name: 'Save search' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Saved search name' }), {
      target: { value: '   ' },
    });
    expect((screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement).disabled).toBe(true);
    expect(usePinnedSearchesStore.getState().searches).toEqual([]);
  });

  it('Cancel closes the form without saving', () => {
    render(<AmbientIsland />);
    expand();
    typeQuery('monitor');

    fireEvent.click(screen.getByRole('button', { name: 'Save search' }));
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(usePinnedSearchesStore.getState().searches).toEqual([]);
    expect(screen.queryByRole('textbox', { name: 'Saved search name' })).toBeNull();
  });

  it('Escape in the name form closes the form and keeps the query', () => {
    render(<AmbientIsland />);
    expand();
    typeQuery('from:ada');

    fireEvent.click(screen.getByRole('button', { name: 'Save search' }));
    fireEvent.keyDown(screen.getByRole('textbox', { name: 'Saved search name' }), {
      key: 'Escape',
    });

    expect(screen.queryByRole('textbox', { name: 'Saved search name' })).toBeNull();
    expect((screen.getByLabelText('Search mail') as HTMLInputElement).value).toBe('from:ada');
    expect(usePinnedSearchesStore.getState().searches).toEqual([]);
  });

  it('blanking the field hides the affordance and any open form', () => {
    render(<AmbientIsland />);
    expand();
    typeQuery('from:ada');
    fireEvent.click(screen.getByRole('button', { name: 'Save search' }));

    fireEvent.keyDown(screen.getByLabelText('Search mail'), { key: 'Escape' });

    expect(screen.queryByRole('button', { name: 'Save search' })).toBeNull();
    expect(screen.queryByRole('textbox', { name: 'Saved search name' })).toBeNull();
    expect(usePinnedSearchesStore.getState().searches).toEqual([]);
  });

  it('captures a search that landed elsewhere through the echo', () => {
    useListStore.setState({
      search: { query: 'monitor', scope: 'current_folder', rows: [], depth_months: 3 },
    });
    render(<AmbientIsland />);

    fireEvent.click(screen.getByRole('button', { name: 'Save search' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(usePinnedSearchesStore.getState().searches[0]).toMatchObject({
      name: 'monitor',
      query: 'monitor',
    });
    expect(Object.keys(usePinnedSearchesStore.getState().searches[0] ?? {}).sort()).toEqual([
      'id',
      'name',
      'query',
    ]);
  });

  it('captures a query that lands through the echo after mount', () => {
    render(<AmbientIsland />);
    expand();

    // A pinned search run from the panel lands the snapshot post-mount: the
    // echo must sync the captured query, not leave the mount-time ref stale.
    act(() => {
      useListStore.setState({
        search: { query: 'invoice', scope: 'all_except_trash', rows: [], depth_months: 3 },
      });
    });

    fireEvent.click(screen.getByRole('button', { name: 'Save search' }));
    const name = screen.getByRole('textbox', { name: 'Saved search name' }) as HTMLInputElement;
    expect(name.value).toBe('invoice');
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(usePinnedSearchesStore.getState().searches[0]).toMatchObject({
      name: 'invoice',
      query: 'invoice',
    });
  });

  it('disables at the cap with its reason', () => {
    usePinnedSearchesStore.setState({
      searches: Array.from({ length: MAX_PINNED_SEARCHES }, (_, i) => ({
        id: `s${i}`,
        name: `Search ${i}`,
        query: `query:${i}`,
      })),
    });
    render(<AmbientIsland />);
    expand();
    typeQuery('monitor');

    const save = screen.getByRole('button', { name: 'Save search' }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    expect(save.title).toBe('You can save up to 20 searches');
  });
});
