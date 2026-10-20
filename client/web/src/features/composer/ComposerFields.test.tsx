// SPDX-License-Identifier: GPL-3.0-only
// The composer fields: From renders as the account picker (each row the
// display name beside the sending address), To and Subject always show,
// Cc and Bcc stay hidden until their toggles open them, a toggle the
// composition cannot carry never stands at all, and every change crosses
// with its exact field name and value.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ComposerFields } from './ComposerFields';

afterEach(cleanup);

/** The account rows the picker draws, the default first. */
const accounts = [
  { id: 'work', label: 'Work', address: 'ada@work.test' },
  { id: 'home', label: 'Home', address: 'ada@home.test' },
];

function setup(over: Partial<Parameters<typeof ComposerFields>[0]> = {}) {
  const onField = vi.fn();
  const onToggleCc = vi.fn();
  const onToggleBcc = vi.fn();
  const onChangeFrom = vi.fn();
  const props = {
    from: 'work',
    fromAccounts: accounts,
    to: 'you@test.local',
    cc: '',
    bcc: '',
    subject: 'Launch',
    ccOffered: true,
    ccShown: false,
    bccOffered: true,
    bccShown: false,
    onField,
    onToggleCc,
    onToggleBcc,
    onChangeFrom,
    ...over,
  };
  render(<ComposerFields {...props} />);
  return { onField, onToggleCc, onToggleBcc, onChangeFrom };
}

describe('the composer fields', () => {
  it('draws From as the account picker, the address beside the name', () => {
    setup();

    const from = screen.getByRole('combobox', { name: 'From' }) as HTMLSelectElement;
    expect(from.value).toBe('work');
    expect(
      Array.from(from.querySelectorAll('option')).map((option) => option.textContent),
    ).toEqual(['Work (ada@work.test)', 'Home (ada@home.test)']);
  });

  it('carries a From change with the chosen account id', () => {
    const { onChangeFrom } = setup();

    fireEvent.change(screen.getByRole('combobox', { name: 'From' }), {
      target: { value: 'home' },
    });

    expect(onChangeFrom).toHaveBeenCalledWith('home');
  });

  it('carries to and subject changes with their field names', () => {
    const { onField } = setup();

    fireEvent.change(screen.getByRole('textbox', { name: 'To' }), {
      target: { value: 'them@test.local' },
    });
    fireEvent.change(screen.getByRole('textbox', { name: 'Subject' }), {
      target: { value: 'Relaunch' },
    });

    expect(onField).toHaveBeenCalledWith('to', 'them@test.local');
    expect(onField).toHaveBeenCalledWith('subject', 'Relaunch');
  });

  it('keeps cc and bcc hidden until their toggles open them', () => {
    const { onToggleCc, onToggleBcc } = setup();

    expect(screen.queryByRole('textbox', { name: 'Cc' })).toBeNull();
    expect(screen.queryByRole('textbox', { name: 'Bcc' })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Show Cc' }));
    fireEvent.click(screen.getByRole('button', { name: 'Show Bcc' }));

    expect(onToggleCc).toHaveBeenCalledTimes(1);
    expect(onToggleBcc).toHaveBeenCalledTimes(1);
  });

  it('withholds a toggle the composition cannot carry', () => {
    setup({ ccOffered: false, bccOffered: false, cc: 'stale@test.local' });

    expect(screen.queryByRole('button', { name: 'Show Cc' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Hide Cc' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Show Bcc' })).toBeNull();
    expect(screen.queryByRole('textbox', { name: 'Cc' })).toBeNull();
    expect(screen.queryByRole('textbox', { name: 'Bcc' })).toBeNull();
  });

  it('shows the cc and bcc inputs once opened and carries their changes', () => {
    const { onField } = setup({ ccShown: true, bccShown: true, cc: 'cc@test.local' });

    expect(screen.getByRole('button', { name: 'Hide Cc' })).toBeTruthy();
    expect((screen.getByRole('textbox', { name: 'Cc' }) as HTMLInputElement).value).toBe(
      'cc@test.local',
    );

    fireEvent.change(screen.getByRole('textbox', { name: 'Cc' }), {
      target: { value: 'other@test.local' },
    });
    fireEvent.change(screen.getByRole('textbox', { name: 'Bcc' }), {
      target: { value: 'quiet@test.local' },
    });

    expect(onField).toHaveBeenCalledWith('cc', 'other@test.local');
    expect(onField).toHaveBeenCalledWith('bcc', 'quiet@test.local');
  });
});
