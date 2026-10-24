// SPDX-License-Identifier: GPL-3.0-only
// The Reading detail: the conversation grouping picker, moved into settings
// through the settings family's own view-mode arm, dispatching its intent
// and reflecting the persisted block; and the swipe action pickers, one per
// direction, held device-local in the appearance store the way the reading
// pane's position is.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { ReadingDetail } from './ReadingDetail';
import { useAppearanceStore } from '@/services/appearance';
import { useSettingsStore } from './settingsStore';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useSettingsStore.getState().applySnapshot({
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
  });
  useAppearanceStore.setState({ swipePrimary: 'archive', swipeSecondary: 'trash' });
});

afterEach(cleanup);

describe('the Reading detail', () => {
  it('dispatches the grouping through the settings family arm', () => {
    render(<ReadingDetail />);
    fireEvent.change(screen.getByLabelText('Grouping'), { target: { value: 'threaded' } });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_view_mode":{"mode":"threaded"}}}',
    );
  });

  it('reflects the persisted grouping, never the click', () => {
    render(<ReadingDetail />);
    fireEvent.change(screen.getByLabelText('Grouping'), { target: { value: 'threaded' } });

    expect((screen.getByLabelText('Grouping') as HTMLSelectElement).value).toBe('flat');
  });
});

describe('the swipe action pickers', () => {
  it('offers each direction its own select over the four acts', () => {
    render(<ReadingDetail />);

    const options = ['Archive', 'Trash', 'Read', 'Flag'];
    const right = screen.getByLabelText('Swipe right') as HTMLSelectElement;
    const left = screen.getByLabelText('Swipe left') as HTMLSelectElement;
    expect(right.value).toBe('archive');
    expect(left.value).toBe('trash');
    for (const select of [right, left]) {
      expect(Array.from(select.options).map((option) => option.textContent)).toEqual(options);
    }
  });

  it('persists each choice through the appearance store, not the settings family', () => {
    render(<ReadingDetail />);

    fireEvent.change(screen.getByLabelText('Swipe right'), { target: { value: 'flag' } });
    fireEvent.change(screen.getByLabelText('Swipe left'), { target: { value: 'read' } });

    expect(useAppearanceStore.getState().swipePrimary).toBe('flag');
    expect(useAppearanceStore.getState().swipeSecondary).toBe('read');
    expect(localStorage.getItem('astra.swipePrimary')).toBe('flag');
    expect(localStorage.getItem('astra.swipeSecondary')).toBe('read');
    expect(ipc.dispatch).toHaveBeenCalledTimes(0);
  });
});
