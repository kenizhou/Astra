// SPDX-License-Identifier: GPL-3.0-only
// The Reading detail: the conversation grouping picker, moved into settings
// through the settings family's own view-mode arm, dispatching its intent
// and reflecting the persisted block.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { ReadingDetail } from './ReadingDetail';
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
