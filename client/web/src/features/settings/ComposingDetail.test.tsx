// SPDX-License-Identifier: GPL-3.0-only
// The Composing detail: the quote-style picker (the quoted original below
// the reply or above it), dispatching its intent and reflecting the
// persisted block.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { ComposingDetail } from './ComposingDetail';
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

describe('the Composing detail', () => {
  it('dispatches the quote-style intent', () => {
    render(<ComposingDetail />);
    fireEvent.change(screen.getByLabelText('Quote style'), { target: { value: 'above' } });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_quote_style":{"below":false}}}',
    );
  });

  it('reflects the persisted style, never the click', () => {
    render(<ComposingDetail />);
    fireEvent.change(screen.getByLabelText('Quote style'), { target: { value: 'above' } });

    expect((screen.getByLabelText('Quote style') as HTMLSelectElement).value).toBe('below');
  });
});
