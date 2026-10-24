// SPDX-License-Identifier: GPL-3.0-only
// The Privacy detail: the plain statement of what stays on the device,
// what the AI features send, and what plugins may ask for; the analytics
// switch, whose off click is the one-click withdrawal and whose on click
// is the consent, one control; and the plugin registry link, which lands
// the settings screen on the Plugins category.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { PrivacyDetail } from './PrivacyDetail';
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

afterEach(() => {
  cleanup();
});

describe('the privacy statement', () => {
  it('states what stays on the device', () => {
    render(<PrivacyDetail />);

    const statement = screen.getByLabelText('Privacy statement');
    expect(statement.textContent).toContain('stay on this device');
    expect(statement.textContent).toContain('counts and shapes');
    expect(statement.textContent).toContain('no message content');
  });

  it('states what the AI features send', () => {
    render(<PrivacyDetail />);

    expect(screen.getByLabelText('Privacy statement').textContent).toContain(
      'model endpoint',
    );
  });

  it('states what plugins may ask for', () => {
    render(<PrivacyDetail />);

    expect(screen.getByLabelText('Privacy statement').textContent).toContain(
      'permissions you grant',
    );
  });
});

describe('the analytics switch', () => {
  it('stands off by default and dispatches the consent arm on a click', () => {
    render(<PrivacyDetail />);

    const box = screen.getByRole('checkbox', { name: 'Product analytics' }) as HTMLInputElement;
    expect(box.checked).toBe(false);

    fireEvent.click(box);
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_analytics_consented":{"consented":true}}}',
    );
    expect(box.checked).toBe(false);
  });

  it('is the one control: the same arm withdraws on its off click', () => {
    useSettingsStore.getState().applySnapshot({
      settings: {
        theme: 'follow_host',
        twenty_four_clock: false,
        zone: 'Etc/UTC',
        first_day_of_week: 'mo',
        horizon_days: 42,
        quote_below: true,
        view_mode: 'flat',
        analytics_consented: true,
      },
      accounts: [],
      about: { version: '0.1.0' },
    });
    render(<PrivacyDetail />);

    const box = screen.getByRole('checkbox', { name: 'Product analytics' }) as HTMLInputElement;
    expect(box.checked).toBe(true);

    fireEvent.click(box);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_analytics_consented":{"consented":false}}}',
    );
  });

  it('states the consent rule beside the switch', () => {
    render(<PrivacyDetail />);

    const note = screen.getByLabelText('Analytics consent note');
    expect(note.textContent).toContain('Off by default');
    expect(note.textContent).toContain('one click');
  });
});

describe('the plugin registry link', () => {
  it('asks the settings screen to land on the Plugins category', () => {
    render(<PrivacyDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Open the plugin registry' }));

    expect(useSettingsStore.getState().requestedCategory).toBe('plugins');
  });

  it('is spendable while the screen stands open', () => {
    render(<PrivacyDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Open the plugin registry' }));

    expect(useSettingsStore.getState().requestedCategory).toBe('plugins');
    // The screen's watched consumption spends the ask and moves the detail;
    // simulated here the way the screen's own effect runs it.
    act(() => {
      useSettingsStore.getState().requestCategory(null);
    });
    expect(useSettingsStore.getState().requestedCategory).toBeNull();
  });
});
