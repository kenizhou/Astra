// SPDX-License-Identifier: GPL-3.0-only
// The reading actions: Reply, Reply all, and Forward dispatch the compose
// family for the message they are handed, each with the exact payload the
// wire pins, and stand disabled with nothing open, so no composition is ever
// asked of a message that is not there.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { ReadingActions } from './ReadingActions';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe('the reading actions', () => {
  it('dispatch each compose intent with the open message exactly', () => {
    render(<ReadingActions message={{ account: 'work', key: 'imap:v1:u1' }} />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Reply' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"compose":{"reply":{"message":{"account":"work","key":"imap:v1:u1"}}}}',
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Reply all' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"compose":{"reply_all":{"message":{"account":"work","key":"imap:v1:u1"}}}}',
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Forward' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[2]?.[0])).toBe(
      '{"compose":{"forward":{"message":{"account":"work","key":"imap:v1:u1"}}}}',
    );
  });

  it('stand disabled with nothing open', () => {
    render(<ReadingActions message={null} />);

    for (const name of ['Reply', 'Reply all', 'Forward']) {
      const button = screen.getByRole('button', { name }) as HTMLButtonElement;
      expect(button.disabled).toBe(true);
    }

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Reply' }));
    });
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });
});
