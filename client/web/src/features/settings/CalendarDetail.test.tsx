// SPDX-License-Identifier: GPL-3.0-only
// The Calendar detail: the zone picker over the browser's own IANA list
// with the app's typed refusal sentence and the field's prior value kept,
// the first-day picker, the horizon picker bounded 7 to 365, and the
// default calendar over the writable calendars the diary holds.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { CalendarDetail } from './CalendarDetail';
import { useCalendarStore } from '@/features/calendar/calendarStore';
import { useSettingsStore } from './settingsStore';
import type { SettingsSnapshot } from '@/services/ipc';

function snapshot(over: Partial<SettingsSnapshot['settings']> = {}): SettingsSnapshot {
  return {
    settings: {
      theme: 'follow_host',
      twenty_four_clock: false,
      zone: 'Europe/Amsterdam',
      first_day_of_week: 'mo',
      horizon_days: 42,
      quote_below: true,
      view_mode: 'flat',
      ...over,
    },
    accounts: [],
    about: { version: '0.1.0' },
  };
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useSettingsStore.getState().applySnapshot(snapshot());
  useCalendarStore.setState({
    calendars: [
      { account: 'work', id: 'cal-1', name: 'Work', visible: true, can_write: true, is_default: true },
      { account: 'home', id: 'cal-2', name: 'Home', visible: true, can_write: true, is_default: false },
      { account: 'work', id: 'cal-3', name: 'Read only', visible: true, can_write: false, is_default: false },
    ],
    occurrences: [],
    writeStates: [],
    replyPrompts: [],
    nowData: null,
    view: 'month',
    anchor: null,
  });
});

afterEach(cleanup);

describe('the zone picker', () => {
  it('offers the browser own zone list with the persisted zone kept on it', () => {
    useSettingsStore.getState().applySnapshot(snapshot({ zone: 'Etc/UTC' }));
    render(<CalendarDetail />);
    const select = screen.getByLabelText('Time zone') as HTMLSelectElement;
    const values = Array.from(select.options).map((option) => option.value);
    // The browser's own list carries the primary zones; the persisted zone
    // stays selectable even when the list omits its spelling.
    expect(values).toContain('Europe/Amsterdam');
    expect(values.length).toBeGreaterThan(100);
    expect(values).toContain('Etc/UTC');
    expect(select.value).toBe('Etc/UTC');
  });

  it('dispatches the zone intent and reflects the persisted block', () => {
    render(<CalendarDetail />);
    fireEvent.change(screen.getByLabelText('Time zone'), { target: { value: 'Europe/London' } });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_zone":{"zone":"Europe/London"}}}',
    );
    expect((screen.getByLabelText('Time zone') as HTMLSelectElement).value).toBe(
      'Europe/Amsterdam',
    );
  });

  it('renders the typed refusal sentence and keeps the prior value', () => {
    render(<CalendarDetail />);
    fireEvent.change(screen.getByLabelText('Time zone'), { target: { value: 'UTC' } });

    act(() => {
      useSettingsStore.getState().applySnapshot({
        ...snapshot(),
        error: 'zone_unresolvable',
      });
    });

    expect(screen.getByRole('alert').textContent).toContain('time zone');
    expect((screen.getByLabelText('Time zone') as HTMLSelectElement).value).toBe(
      'Europe/Amsterdam',
    );
  });
});

describe('the first-day picker', () => {
  it('dispatches the weekday intent and reflects the block', () => {
    render(<CalendarDetail />);
    fireEvent.change(screen.getByLabelText('First day of week'), { target: { value: 'su' } });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_first_day_of_week":{"weekday":"su"}}}',
    );
    expect((screen.getByLabelText('First day of week') as HTMLSelectElement).value).toBe('mo');
  });
});

describe('the horizon picker', () => {
  it('dispatches the horizon intent inside the 7 to 365 bound', () => {
    render(<CalendarDetail />);
    fireEvent.change(screen.getByLabelText('Horizon days'), { target: { value: '365' } });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_horizon":{"days":365}}}',
    );
  });

  it('never dispatches outside the bound', () => {
    render(<CalendarDetail />);
    fireEvent.change(screen.getByLabelText('Horizon days'), { target: { value: '5' } });
    fireEvent.change(screen.getByLabelText('Horizon days'), { target: { value: '400' } });

    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect((screen.getByLabelText('Horizon days') as HTMLInputElement).value).toBe('42');
  });
});

describe('the default calendar picker', () => {
  it('lists the writable calendars and None, and reflects the pin', () => {
    useSettingsStore.getState().applySnapshot(
      snapshot({ default_calendar: { account: 'home', key: 'cal-2' } }),
    );
    render(<CalendarDetail />);

    const select = screen.getByLabelText('Default calendar') as HTMLSelectElement;
    const values = Array.from(select.options).map((option) => option.value);
    expect(values).toEqual(['', 'work:cal-1', 'home:cal-2']);
    expect(select.value).toBe('home:cal-2');
  });

  it('dispatches the pin and the unpin', () => {
    render(<CalendarDetail />);
    fireEvent.change(screen.getByLabelText('Default calendar'), { target: { value: 'work:cal-1' } });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_default_calendar":{"calendar":{"account":"work","key":"cal-1"}}}}',
    );

    ipc.dispatch.mockClear();
    fireEvent.change(screen.getByLabelText('Default calendar'), { target: { value: '' } });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_default_calendar":{"calendar":null}}}',
    );
  });
});
