// SPDX-License-Identifier: GPL-3.0-only
// The list toolbar's filter control: four segmented buttons carrying the
// counts of the snapshot's rows per tab, the pressed state following the
// chosen filter, and every click crossing back with the tab it names.
// The search header beside them: the depth note stated at a nonzero
// horizon and absent at zero, where the search claims no horizon at all.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { SearchState } from '@/services/surfaces';
import { ListToolbar } from './ListToolbar';

afterEach(cleanup);

const COUNTS = { all: 12, unread: 5, flagged: 3, attachments: 2 };

function renderToolbar(overrides: Partial<Parameters<typeof ListToolbar>[0]> = {}) {
  return render(
    <ListToolbar
      folderName="Inbox"
      viewMode="flat"
      filter="all"
      filterCounts={COUNTS}
      onFilter={() => undefined}
      search={null}
      {...overrides}
    />,
  );
}

describe('the filter control', () => {
  it('renders the four tabs with their count badges', () => {
    renderToolbar();

    expect(screen.getByRole('group', { name: 'Filter' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'All 12' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Unread 5' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Flagged 3' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Attachments 2' })).toBeTruthy();
  });

  it('presses the chosen tab alone', () => {
    renderToolbar({ filter: 'flagged' });

    expect(screen.getByRole('button', { name: 'All 12' }).getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByRole('button', { name: 'Unread 5' }).getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByRole('button', { name: 'Flagged 3' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'Attachments 2' }).getAttribute('aria-pressed')).toBe('false');
  });

  it('crosses the clicked tab back', () => {
    const onFilter = vi.fn();
    renderToolbar({ onFilter });

    fireEvent.click(screen.getByRole('button', { name: 'Unread 5' }));
    expect(onFilter).toHaveBeenCalledWith('unread');

    fireEvent.click(screen.getByRole('button', { name: 'Flagged 3' }));
    expect(onFilter).toHaveBeenCalledWith('flagged');

    fireEvent.click(screen.getByRole('button', { name: 'Attachments 2' }));
    expect(onFilter).toHaveBeenCalledWith('attachments');

    fireEvent.click(screen.getByRole('button', { name: 'All 12' }));
    expect(onFilter).toHaveBeenCalledWith('all');
  });

  it('keeps the view mode control beside the filter', () => {
    renderToolbar({ viewMode: 'threaded' });

    expect(screen.getByRole('button', { name: 'Threaded' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'All 12' })).toBeTruthy();
  });

  it('hides the filter while a search is live', () => {
    renderToolbar({ search: { query: 'compiler', scope: 'all_except_trash', rows: [], depth_months: 0 } });

    expect(screen.queryByRole('group', { name: 'Filter' })).toBeNull();
  });
});

describe('the search header', () => {
  /** A live search state with the given horizon and nothing found. */
  const liveSearch = (depth_months: number): SearchState => ({
    query: 'compiler',
    scope: 'all_except_trash',
    rows: [],
    depth_months,
  });

  it('states the singular depth note at a one-month horizon', () => {
    renderToolbar({ search: liveSearch(1) });

    expect(screen.getByText('0 results')).toBeTruthy();
    expect(screen.getByText('Searched the last month')).toBeTruthy();
  });

  it('states no depth note at a zero horizon', () => {
    renderToolbar({ search: liveSearch(0) });

    expect(screen.getByText('0 results')).toBeTruthy();
    expect(screen.queryByText(/^Searched the last/)).toBeNull();
  });
});
