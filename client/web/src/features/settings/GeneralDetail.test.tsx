// SPDX-License-Identifier: GPL-3.0-only
// The General detail: the theme picker (follow host, Light, Dark) and the
// clock picker (24 or 12 hour), each dispatching its intent and reflecting
// the snapshot's persisted truth, never local optimism, and the persist
// failure's sentence under the controls. The appearance choices (accent,
// contrast, font size) are device-local: they write localStorage and the
// document-root attributes and dispatch no intent.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { GeneralDetail } from './GeneralDetail';
import { useSettingsStore } from './settingsStore';
import { useAppearanceStore } from '@/services/appearance';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  localStorage.clear();
  const root = document.documentElement;
  delete root.dataset.skin;
  delete root.dataset.contrast;
  delete root.dataset.fontSize;
  useAppearanceStore.setState({ skin: 'iris', contrast: 'default', fontSize: 'default' });
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

afterEach(() => {
  cleanup();
  localStorage.clear();
});

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

  it('marks the standing accent and moves the mark on a click', () => {
    render(<GeneralDetail />);

    expect(screen.getByRole('button', { name: 'Iris accent' }).getAttribute('aria-pressed')).toBe(
      'true',
    );

    fireEvent.click(screen.getByRole('button', { name: 'Emerald accent' }));

    expect(screen.getByRole('button', { name: 'Emerald accent' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    expect(document.documentElement.dataset.skin).toBe('emerald');
    expect(localStorage.getItem('astra.skin')).toBe('emerald');
    // Device-local: no intent crosses the wire.
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('applies the contrast choice to the document root and the store', () => {
    render(<GeneralDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'High' }));

    expect(document.documentElement.dataset.contrast).toBe('high');
    expect(localStorage.getItem('astra.contrast')).toBe('high');
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('applies the font-size step to the document root and the store', () => {
    render(<GeneralDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Large' }));

    expect(document.documentElement.dataset.fontSize).toBe('large');
    expect(localStorage.getItem('astra.fontSize')).toBe('large');
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });
});

describe('the density control', () => {
  it('writes the density through the appearance store from the segmented row', () => {
    render(<GeneralDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Compact' }));

    expect(useAppearanceStore.getState().density).toBe('compact');
    expect(localStorage.getItem('astra.density')).toBe('compact');
    expect(document.documentElement.dataset.density).toBe('compact');
  });

  it('renders the persisted density as pressed', () => {
    useAppearanceStore.getState().setDensity('compact');
    render(<GeneralDetail />);

    expect(screen.getByRole('button', { name: 'Compact' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'Comfortable' }).getAttribute('aria-pressed')).toBe('false');
  });
});
