// SPDX-License-Identifier: GPL-3.0-only
// The outgoing-name card's own focused tests: the ask-state transition
// the page can see when a late adoption answers the ask while the card
// stands. The three controls' behaviour is pinned over the accounts page
// in AccountsDetail.test.tsx, where the card is mounted in place.
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { SenderNameCard } from './SenderNameCard';
import type { AccountSettingsRow } from '@/services/ipc';

/** A minimal account row the card reads, overridable per test. */
function row(over: Partial<AccountSettingsRow> = {}): AccountSettingsRow {
  return {
    account: 'work',
    address: 'ada@example.test',
    display_name: 'ada@example.test',
    sender_name_control: 'writable',
    needs_sender_name: false,
    sender_name_error: null,
    ...over,
  } as AccountSettingsRow;
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
});

describe('the outgoing-name card', () => {
  it('reseeds the field when a late adoption answers the ask', async () => {
    // The ask stands on the page, its field empty; the join's identity
    // read lands a name and republishes: the card flips to the controls
    // with the adopted name IN the field, so one unthinking Save cannot
    // clobber the adopted name to the bare address.
    const initial = row({ needs_sender_name: true });
    const { rerender } = render(<SenderNameCard row={initial} />);
    const before = screen.getByRole('textbox') as HTMLInputElement;
    expect(before.value).toBe('');

    const adopted = row({
      needs_sender_name: false,
      display_name: 'Ada Lovelace',
    });
    await act(async () => {
      rerender(<SenderNameCard row={adopted} />);
    });

    const field = screen.getByRole('textbox') as HTMLInputElement;
    expect(field.value).toBe('Ada Lovelace');
  });
});
