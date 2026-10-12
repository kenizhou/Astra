// SPDX-License-Identifier: GPL-3.0-only
// The General detail: the theme picker (follow host, Light, Dark) and the
// clock picker (24 or 12 hour), each dispatching its intent and reflecting
// the snapshot's persisted truth, never local optimism, and the persist
// failure's sentence under the controls.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { GeneralDetail } from './GeneralDetail';
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

describe('the General detail', () => {
  it('dispatches the theme intent from the picker', () => {
    render(<GeneralDetail />);
    fireEvent.change(screen.getByLabelText('Theme'), { target: { value: 'dark' } });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_theme":{"mode":"dark"}}}',
    );
  });

  it('reflects the snapshot theme, not the click that started the write', () => {
    render(<GeneralDetail />);
    fireEvent.change(screen.getByLabelText('Theme'), { target: { value: 'dark' } });

    // No snapshot landed, so the control still shows the persisted mode.
    expect((screen.getByLabelText('Theme') as HTMLSelectElement).value).toBe('follow_host');

    act(() => {
      useSettingsStore.getState().applySnapshot({
        settings: {
          theme: 'dark',
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
    expect((screen.getByLabelText('Theme') as HTMLSelectElement).value).toBe('dark');
  });

  it('dispatches the clock intent and reflects the persisted block', () => {
    render(<GeneralDetail />);
    fireEvent.change(screen.getByLabelText('Clock'), { target: { value: '24' } });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_clock":{"twenty_four":true}}}',
    );
    expect((screen.getByLabelText('Clock') as HTMLSelectElement).value).toBe('12');
  });

  it('renders the persist failure sentence when the snapshot carries it', () => {
    useSettingsStore.getState().applySnapshot({
      settings: useSettingsStore.getState().settings,
      accounts: [],
      about: { version: '0.1.0' },
      error: 'persist_failed',
    });
    render(<GeneralDetail />);

    expect(screen.getByRole('alert').textContent).toContain('could not be saved');
  });
});
