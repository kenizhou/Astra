// SPDX-License-Identifier: GPL-3.0-only
// The composer fields: From renders read only, To and Subject always show,
// Cc and Bcc stay hidden until their toggles open them, and every change
// crosses with its exact field name and value.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ComposerFields } from './ComposerFields';

afterEach(cleanup);

function setup(over: Partial<Parameters<typeof ComposerFields>[0]> = {}) {
  const onField = vi.fn();
  const onToggleCc = vi.fn();
  const onToggleBcc = vi.fn();
  const props = {
    fromLabel: 'Work',
    to: 'you@test.local',
    cc: '',
    bcc: '',
    subject: 'Launch',
    ccShown: false,
    bccShown: false,
    onField,
    onToggleCc,
    onToggleBcc,
    ...over,
  };
  render(<ComposerFields {...props} />);
  return { onField, onToggleCc, onToggleBcc };
}

describe('the composer fields', () => {
  it('shows from as read only text', () => {
    setup();

    expect(screen.getByText('Work')).toBeTruthy();
    expect(screen.queryByRole('textbox', { name: 'From' })).toBeNull();
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
