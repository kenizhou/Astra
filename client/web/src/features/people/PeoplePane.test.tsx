// SPDX-License-Identifier: GPL-3.0-only
// The people pane: the refresh on entry and on focus regain, the search box
// debouncing its query into the Search intent, the write statuses beside the
// list (never inside the detail pane), the create affordance standing only
// where a writable book exists, and the row tap opening the person.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), peopleCard: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  peopleCard: ipc.peopleCard,
}));

import type { PeopleSnapshot } from '@/services/ipc';

import { PeoplePane } from './PeoplePane';
import { usePeopleStore } from './peopleStore';

function snapshot(over: Partial<PeopleSnapshot> = {}): PeopleSnapshot {
  return {
    rows: [
      {
        person: 7,
        display_name: 'Ada Lovelace',
        monogram: { initials: 'AL', colour: '#4DB6AC' },
        section: 'A',
        primary_address: 'ada@example.test',
      },
    ],
    ...over,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.peopleCard.mockReset();
  ipc.peopleCard.mockResolvedValue(null);
  usePeopleStore.getState().applySnapshot(snapshot());
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('the people pane', () => {
  it('refreshes on entry and again when the window regains focus', () => {
    render(<PeoplePane />);
    const entry = ipc.dispatch.mock.calls.filter(
      (call) => JSON.stringify(call[0]) === '{"people":"refresh"}',
    ).length;
    expect(entry).toBe(1);

    act(() => {
      window.dispatchEvent(new Event('focus'));
    });

    const regained = ipc.dispatch.mock.calls.filter(
      (call) => JSON.stringify(call[0]) === '{"people":"refresh"}',
    ).length;
    expect(regained).toBe(2);
  });

  it('debounces the search box into the Search intent', () => {
    render(<PeoplePane />);

    fireEvent.change(screen.getByLabelText('Search people'), { target: { value: 'a' } });
    fireEvent.change(screen.getByLabelText('Search people'), { target: { value: 'ad' } });
    const searchesBefore = ipc.dispatch.mock.calls
      .map((call) => JSON.stringify(call[0]))
      .filter((body) => body.includes('search'));
    expect(searchesBefore).toHaveLength(0);

    act(() => {
      vi.advanceTimersByTime(500);
    });

    const searches = ipc.dispatch.mock.calls
      .map((call) => JSON.stringify(call[0]))
      .filter((body) => body.includes('search'));
    expect(searches).toEqual(['{"people":{"search":{"query":"ad"}}}']);
  });

  it('opens the person whose row is tapped and closes the detail again', () => {
    render(<PeoplePane />);

    fireEvent.click(screen.getByText('Ada Lovelace'));
    expect(JSON.stringify(ipc.dispatch.mock.calls.at(-1)?.[0])).toBe(
      '{"people":{"open_person":{"person":7}}}',
    );

    act(() => {
      usePeopleStore.getState().applySnapshot({
        ...snapshot(),
        detail: {
          person: 7,
          display_name: 'Ada Lovelace',
          values: [{ kind: 'email', value: 'ada@example.test', sources: [] }],
        },
      });
    });
    fireEvent.click(screen.getByRole('button', { name: 'Close person' }));
    expect(JSON.stringify(ipc.dispatch.mock.calls.at(-1)?.[0])).toBe('{"people":"close_person"}');
  });

  it('draws the writing status beside the list, never inside the detail pane', () => {
    usePeopleStore.getState().applySnapshot({
      ...snapshot(),
      write_states: [{ account: 'work', state: 'writing' }],
      detail: {
        person: 7,
        display_name: 'Ada Lovelace',
        values: [],
      },
    });
    render(<PeoplePane />);

    const banner = screen.getByText('Saving contacts…');
    expect(banner.closest('aside')?.getAttribute('aria-label')).toBe('People list');
  });

  it('draws the failed status with its retry beside the list', () => {
    usePeopleStore.getState().applySnapshot({
      ...snapshot(),
      write_states: [{ account: 'work', state: 'failed_with_retry' }],
    });
    render(<PeoplePane />);

    expect(
      screen.getByText('The last contact change did not settle. Refresh to see what stood.'),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    const refresh = ipc.dispatch.mock.calls.filter(
      (call) => JSON.stringify(call[0]) === '{"people":"refresh"}',
    ).length;
    expect(refresh).toBeGreaterThanOrEqual(2);
  });

  it('offers the create affordance only where a writable book stands', () => {
    const { unmount } = render(<PeoplePane />);
    expect(screen.queryByRole('button', { name: 'New contact' })).toBeNull();
    unmount();

    usePeopleStore.getState().applySnapshot({
      ...snapshot(),
      create_targets: [{ account: 'work', book: '/dav/book/', name: 'Contacts', is_default: true }],
    });
    render(<PeoplePane />);

    fireEvent.click(screen.getByRole('button', { name: 'New contact' }));
    expect(screen.getByRole('dialog', { name: 'New contact' })).toBeTruthy();
  });
});
