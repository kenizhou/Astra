// SPDX-License-Identifier: GPL-3.0-only
// The list toolbar's filter control: four segmented buttons carrying the
// counts of the snapshot's rows per tab, the pressed state following the
// chosen filter, and every click crossing back with the tab it names.
// The search header beside them: the depth note stated at a nonzero
// horizon and absent at zero, where the search claims no horizon at all.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import type { MessageRowView, SearchState } from '@/services/surfaces';
import { ListToolbar } from './ListToolbar';

afterEach(() => {
  cleanup();
  ipc.dispatch.mockReset();
});

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

/** One selected message row for the bulk bar's actions. */
function row(key: string, unread = false, flagged = false): MessageRowView {
  return {
    key: { message: { account: 'work', key } },
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'Report',
    sent_at: '2026-10-08T16:05:00Z',
    unread,
    flagged,
    has_attachments: false,
  };
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

describe('the bulk bar', () => {
  const bulk = {
    rows: [row('m1', true), row('m2'), row('m3')],
    onSelectAll: vi.fn(),
    onClear: vi.fn(),
  };

  it('draws nothing while no selection stands', () => {
    renderToolbar();
    expect(screen.queryByRole('status')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Clear' })).toBeNull();
  });

  it('states the count with Select All and Clear while a selection stands', () => {
    renderToolbar({ bulk });

    expect(screen.getByText('3 selected')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Select All' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Clear' })).toBeTruthy();
  });

  it('hands Select All and Clear to the list', () => {
    renderToolbar({ bulk });

    fireEvent.click(screen.getByRole('button', { name: 'Select All' }));
    expect(bulk.onSelectAll).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    expect(bulk.onClear).toHaveBeenCalledTimes(1);
  });

  it('loops the batch actions per row exactly as the row menu does', () => {
    renderToolbar({ bulk });

    fireEvent.click(screen.getByRole('button', { name: 'Mark as Read' }));
    expect(ipc.dispatch).toHaveBeenCalledTimes(3);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      mark_read: { row: { message: { account: 'work', key: 'm1' } }, read: true },
    });

    ipc.dispatch.mockClear();
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(ipc.dispatch).toHaveBeenCalledWith({
      move_to_trash: { row: { message: { account: 'work', key: 'm3' } } },
    });

    ipc.dispatch.mockClear();
    fireEvent.click(screen.getByRole('button', { name: 'Flag' }));
    expect(ipc.dispatch).toHaveBeenCalledWith({
      set_flagged: { row: { message: { account: 'work', key: 'm2' } }, flagged: true },
    });
  });
});
