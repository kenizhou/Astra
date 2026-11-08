// SPDX-License-Identifier: GPL-3.0-only
// The Diagnostics detail: the Read button pulls the tail (200 lines, oldest
// first, so the newest lands last in the monospace block), Copy for support
// writes the tail beside the version, Export surfaces the save outcome, the
// debug toggle dispatches the settings arm and states the next-launch rule,
// and the privacy note names what the log never carries.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));
const diagnostics = vi.hoisted(() => ({
  logTail: vi.fn(),
  logExport: vi.fn(),
}));
const clipboard = vi.hoisted(() => ({ writeText: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));
vi.mock('@/services/diagnostics', () => ({
  logTail: diagnostics.logTail,
  logExport: diagnostics.logExport,
  TAIL_LINES: 200,
}));

import { DiagnosticsDetail } from './DiagnosticsDetail';
import { useSettingsStore } from './settingsStore';

const writeText = clipboard.writeText;

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  diagnostics.logTail.mockReset();
  diagnostics.logTail.mockResolvedValue([]);
  diagnostics.logExport.mockReset();
  diagnostics.logExport.mockResolvedValue('cancelled');
  writeText.mockReset();
  writeText.mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', {
    value: { writeText },
    configurable: true,
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
    about: { version: '0.2.7' },
  });
});

afterEach(() => {
  cleanup();
});

describe('the log tail viewer', () => {
  it('pulls 200 lines on Read and renders them newest last', async () => {
    diagnostics.logTail.mockResolvedValue(['oldest line', 'newest line']);
    render(<DiagnosticsDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Read the log' }));

    await waitFor(() => expect(diagnostics.logTail).toHaveBeenCalledWith(200));
    const block = await screen.findByLabelText('Log tail');
    expect(block.textContent).toBe('oldest line\nnewest line');
    expect(block.tagName).toBe('PRE');
  });

  it('leaves the block empty when the tail answers nothing', async () => {
    render(<DiagnosticsDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Read the log' }));

    expect((await screen.findByLabelText('Log tail')).textContent).toBe('');
  });
});

describe('copy for support', () => {
  it('writes the tail beside the version', async () => {
    diagnostics.logTail.mockResolvedValue(['a line']);
    render(<DiagnosticsDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Copy for support' }));

    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText.mock.calls[0]?.[0]).toBe('a line\nAstra Mail 0.2.7');
    expect(await screen.findByText(/Copied/)).toBeTruthy();
  });

  it('copies the tail it already holds, spending no second pull', async () => {
    diagnostics.logTail.mockResolvedValue(['first']);
    render(<DiagnosticsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Read the log' }));
    await screen.findByLabelText('Log tail');

    diagnostics.logTail.mockClear();
    fireEvent.click(screen.getByRole('button', { name: 'Copy for support' }));

    await waitFor(() => expect(writeText).toHaveBeenCalledWith('first\nAstra Mail 0.2.7'));
    expect(diagnostics.logTail).not.toHaveBeenCalled();
  });

  it('says so when the clipboard refuses', async () => {
    diagnostics.logTail.mockResolvedValue(['a line']);
    writeText.mockRejectedValue(new Error('the clipboard refused'));
    render(<DiagnosticsDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Copy for support' }));

    expect(await screen.findByRole('alert')).toBeTruthy();
  });
});

describe('the export', () => {
  it('surfaces the saved path', async () => {
    diagnostics.logExport.mockResolvedValue({ saved: { path: 'C:\\Users\\ada\\astra.log' } });
    render(<DiagnosticsDetail />);

    fireEvent.click(screen.getByRole('button', { name: 'Export the log' }));

    expect(await screen.findByText(/Exported to/)).toBeTruthy();
  });

  it('surfaces a failure sentence and stays quiet on a cancel', async () => {
    diagnostics.logExport.mockResolvedValue({ failed: { reason: 'the disk refused' } });
    render(<DiagnosticsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Export the log' }));
    expect(await screen.findByText(/the disk refused/)).toBeTruthy();

    cleanup();
    diagnostics.logExport.mockResolvedValue('cancelled');
    render(<DiagnosticsDetail />);
    fireEvent.click(screen.getByRole('button', { name: 'Export the log' }));
    await waitFor(() => expect(diagnostics.logExport).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

describe('the debug toggle', () => {
  it('stands off by default and dispatches the settings arm', () => {
    render(<DiagnosticsDetail />);

    const box = screen.getByRole('checkbox', { name: 'Debug logging' }) as HTMLInputElement;
    expect(box.checked).toBe(false);

    fireEvent.click(box);
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_debug_enabled":{"enabled":true}}}',
    );
    expect(box.checked).toBe(false);
  });

  it('reflects the persisted block and states the next-launch rule', () => {
    useSettingsStore.getState().applySnapshot({
      settings: {
        theme: 'follow_host',
        twenty_four_clock: false,
        zone: 'Etc/UTC',
        first_day_of_week: 'mo',
        horizon_days: 42,
        quote_below: true,
        view_mode: 'flat',
        debug_enabled: true,
      },
      accounts: [],
      about: { version: '0.2.7' },
    });
    render(<DiagnosticsDetail />);

    expect(
      (screen.getByRole('checkbox', { name: 'Debug logging' }) as HTMLInputElement).checked,
    ).toBe(true);
    expect(screen.getByText('takes effect the next time Astra Mail starts')).toBeTruthy();
  });

  it('names what the log never carries', () => {
    render(<DiagnosticsDetail />);

    const note = screen.getByText(/no message content/);
    expect(note.textContent).toContain('no credentials');
  });
});
