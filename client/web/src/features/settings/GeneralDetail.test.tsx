// SPDX-License-Identifier: GPL-3.0-only
// The General detail: the theme picker (follow host, Light, Dark) and the
// clock picker (24 or 12 hour), each dispatching its intent and reflecting
// the snapshot's persisted truth, never local optimism, and the persist
// failure's sentence under the controls. The appearance choices (accent,
// contrast, font size) are device-local: they write localStorage and the
// document-root attributes and dispatch no intent.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));
const platform = vi.hoisted(() => ({ registerDefaultMail: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));
vi.mock('@/services/platform', () => ({
  registerDefaultMail: platform.registerDefaultMail,
}));

import { GeneralDetail } from './GeneralDetail';
import { useSettingsStore } from './settingsStore';
import { useAppearanceStore } from '@/services/appearance';
import { useAccountStore } from '@/stores/accountStore';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  platform.registerDefaultMail.mockReset();
  platform.registerDefaultMail.mockResolvedValue('registered');
  localStorage.clear();
  act(() => {
    useAccountStore.getState().applySnapshot({
      account_folders: [
        {
          account_id: 'work',
          display_name: 'Ada',
          address: 'ada@example.test',
          expanded: false,
          folders: [],
        },
      ],
    });
  });
  const root = document.documentElement;
  delete root.dataset.skin;
  delete root.dataset.contrast;
  delete root.dataset.fontSize;
  delete root.dataset.density;
  delete root.dataset.texture;
  useAppearanceStore.setState({
    skin: 'iris',
    contrast: 'default',
    fontSize: 'default',
    density: 'comfortable',
    texture: 'warm',
  });
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

describe('the notifications switch', () => {
  it('stands on by default, the block carrying no field', () => {
    render(<GeneralDetail />);

    expect((screen.getByRole('checkbox', { name: 'New mail notifications' }) as HTMLInputElement)
      .checked).toBe(true);
  });

  it('reflects the persisted block, never local optimism', () => {
    useSettingsStore.getState().applySnapshot({
      settings: {
        theme: 'follow_host',
        twenty_four_clock: false,
        zone: 'Etc/UTC',
        first_day_of_week: 'mo',
        horizon_days: 42,
        quote_below: true,
        view_mode: 'flat',
        notifications_enabled: false,
      },
      accounts: [],
      about: { version: '0.1.0' },
    });
    render(<GeneralDetail />);

    const box = screen.getByRole('checkbox', {
      name: 'New mail notifications',
    }) as HTMLInputElement;
    expect(box.checked).toBe(false);

    fireEvent.click(box);
    expect(box.checked).toBe(false);
  });

  it('writes its payload through the settings family when toggled', () => {
    render(<GeneralDetail />);

    fireEvent.click(screen.getByRole('checkbox', { name: 'New mail notifications' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_notifications_enabled":{"enabled":false}}}',
    );
  });
});

describe('the texture control', () => {
  it('writes the texture through the appearance store from the segmented row', () => {
    render(<GeneralDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Calm' }));

    expect(useAppearanceStore.getState().texture).toBe('calm');
    expect(localStorage.getItem('astra.texture')).toBe('calm');
    expect(document.documentElement.dataset.texture).toBe('calm');
    // Device-local: no intent crosses the wire.
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('renders warm as the default and the persisted texture as pressed', () => {
    render(<GeneralDetail />);

    expect(screen.getByRole('button', { name: 'Warm' }).getAttribute('aria-pressed')).toBe('true');
    cleanup();

    useAppearanceStore.getState().setTexture('classic');
    render(<GeneralDetail />);

    expect(screen.getByRole('button', { name: 'Classic' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    expect(screen.getByRole('button', { name: 'Warm' }).getAttribute('aria-pressed')).toBe('false');
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

describe('the language picker', () => {
  it('offers the one catalogue that ships and follows the persisted tag', () => {
    render(<GeneralDetail />);

    const picker = screen.getByLabelText('Language') as HTMLSelectElement;
    expect(Array.from(picker.options).map((option) => option.value)).toEqual(['en-GB']);
    expect(picker.value).toBe('en-GB');
  });

  it('dispatches the language arm on a choice', () => {
    render(<GeneralDetail />);

    fireEvent.change(screen.getByLabelText('Language'), { target: { value: 'en-GB' } });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_language":{"language":"en-GB"}}}',
    );
  });

  it('states honestly what follows the tag today', () => {
    render(<GeneralDetail />);

    expect(
      screen.getByText('Dates and the first localised surfaces follow it today.'),
    ).toBeTruthy();
  });
});

describe('the default-mail setting', () => {
  it('re-asserts the registration through the host verb', async () => {
    render(<GeneralDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Make Astra Mail the default mail app' }));

    await waitFor(() => expect(platform.registerDefaultMail).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/is the default mail app/)).toBeTruthy();
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('surfaces an already-standing registration', async () => {
    platform.registerDefaultMail.mockResolvedValue('already_default');
    render(<GeneralDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Make Astra Mail the default mail app' }));

    expect(await screen.findByText(/already/)).toBeTruthy();
  });

  it('surfaces a refusal and hides the row when no account stands', async () => {
    platform.registerDefaultMail.mockResolvedValue({ failed: { reason: 'the registry refused' } });
    render(<GeneralDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Make Astra Mail the default mail app' }));
    expect(await screen.findByText(/the registry refused/)).toBeTruthy();

    cleanup();
    act(() => {
      useAccountStore.getState().applySnapshot({ account_folders: [] });
    });
    render(<GeneralDetail />);

    expect(
      screen.queryByRole('button', { name: 'Make Astra Mail the default mail app' }),
    ).toBeNull();
  });
});

describe('the reading pane control', () => {
  it('writes the position through the appearance store from the segmented row', () => {
    render(<GeneralDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Bottom' }));

    expect(useAppearanceStore.getState().readingPosition).toBe('bottom');
    expect(localStorage.getItem('astra.readingPosition')).toBe('bottom');
  });

  it('renders the persisted position as pressed', () => {
    useAppearanceStore.getState().setReadingPosition('off');
    render(<GeneralDetail />);

    expect(screen.getByRole('button', { name: 'Off' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'Right' }).getAttribute('aria-pressed')).toBe('false');
    useAppearanceStore.getState().setReadingPosition('right');
  });
});
