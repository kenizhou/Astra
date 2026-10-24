// SPDX-License-Identifier: GPL-3.0-only
// The diary's device-zone offer: a prompt over the calendar naming the
// offered zone with Use it and Keep {current}; the accept dispatches the
// acceptance arm, the keep dismisses the standing offer, and no prompt
// stands when the snapshot carries no offer.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { DeviceZoneOffer } from './DeviceZoneOffer';
import { useSettingsStore } from '@/features/settings/settingsStore';

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

afterEach(() => {
  cleanup();
});

describe('the device-zone offer', () => {
  it('stays hidden when no offer stands', () => {
    render(<DeviceZoneOffer />);
    expect(screen.queryByRole('dialog', { name: 'Device time zone offer' })).toBeNull();
  });

  it('names the offered zone beside the standing one', () => {
    act(() => {
      useSettingsStore.getState().applySnapshot({
        settings: useSettingsStore.getState().settings,
        accounts: [],
        about: { version: '0.1.0' },
        device_zone_offer: 'Asia/Shanghai',
      });
    });
    render(<DeviceZoneOffer />);

    const prompt = screen.getByRole('dialog', { name: 'Device time zone offer' });
    expect(prompt.textContent).toContain('Asia/Shanghai');
    expect(screen.getByRole('button', { name: 'Keep Etc/UTC' })).toBeTruthy();
  });

  it('dispatches the acceptance arm on Use it', () => {
    act(() => {
      useSettingsStore.getState().applySnapshot({
        settings: useSettingsStore.getState().settings,
        accounts: [],
        about: { version: '0.1.0' },
        device_zone_offer: 'Asia/Shanghai',
      });
    });
    render(<DeviceZoneOffer />);

    fireEvent.click(screen.getByRole('button', { name: 'Use it' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"accept_device_zone":{"zone":"Asia/Shanghai"}}}',
    );
  });

  it('dismisses the standing offer on Keep', () => {
    act(() => {
      useSettingsStore.getState().applySnapshot({
        settings: useSettingsStore.getState().settings,
        accounts: [],
        about: { version: '0.1.0' },
        device_zone_offer: 'Asia/Shanghai',
      });
    });
    render(<DeviceZoneOffer />);

    fireEvent.click(screen.getByRole('button', { name: 'Keep Etc/UTC' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":"dismiss_device_zone"}',
    );
  });
});
