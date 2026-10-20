// SPDX-License-Identifier: GPL-3.0-only
// The Composing detail: the quote-style picker (the quoted original below
// the reply or above it), the app-level default send account, and the
// signature editor (rich text, per-account new mail and reply slots), each
// dispatching its intent and reflecting the persisted block, never local
// optimism.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { ComposingDetail } from './ComposingDetail';
import { useSettingsStore } from './settingsStore';

const workSignature = { body_html: '<p>Ada</p>', body_plain: 'Ada' };

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
    accounts: [
      {
        account: 'work',
        address: 'ada@work.test',
        kind: 'imap_smtp',
        legs: { calendar: false, contacts: false },
        signatures: { new_mail: workSignature },
      },
      {
        account: 'home',
        address: 'ada@home.test',
        kind: 'imap_smtp',
        legs: { calendar: false, contacts: false },
      },
    ],
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

describe('the default send account', () => {
  it('lists the accounts by their addresses and dispatches the pin', () => {
    render(<ComposingDetail />);
    const picker = screen.getByLabelText('Default send account') as HTMLSelectElement;

    expect(
      Array.from(picker.querySelectorAll('option')).map((option) => option.textContent),
    ).toEqual(['First account', 'ada@work.test', 'ada@home.test']);
    expect(picker.value).toBe('');

    fireEvent.change(picker, { target: { value: 'home' } });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_send_account":{"account":"home"}}}',
    );
  });

  it('unpins through the first-account choice', () => {
    useSettingsStore.setState({
      settings: { ...useSettingsStore.getState().settings, default_send_account: 'home' },
    });
    render(<ComposingDetail />);

    const picker = screen.getByLabelText('Default send account') as HTMLSelectElement;
    expect(picker.value).toBe('home');
    fireEvent.change(picker, { target: { value: '' } });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_send_account":{"account":null}}}',
    );
  });
});

describe('the signature editor', () => {
  it('seeds the editor from the chosen account and slot', async () => {
    render(<ComposingDetail />);

    const body = await screen.findByLabelText('Message body');
    expect(body.textContent).toContain('Ada');

    fireEvent.click(screen.getByRole('button', { name: 'Reply' }));
    expect(screen.getByLabelText('Message body').textContent).not.toContain('Ada');
  });

  it('saves the edited words into the chosen slot with both legs', async () => {
    render(<ComposingDetail />);
    const body = await screen.findByLabelText('Message body');
    body.focus();
    // The words stand selected the way a person selects them, then a mark
    // applies over the selection the toolbar reflects.
    await waitFor(() => {
      fireEvent.keyDown(body, { key: 'a', ctrlKey: true });
      fireEvent.click(screen.getByRole('button', { name: 'Bold' }));
    });

    fireEvent.click(screen.getByRole('button', { name: 'Save signature' }));

    const sent = ipc.dispatch.mock.calls[0]?.[0] as {
      settings: { set_signature: { account: string; reply: boolean; signature: unknown } };
    };
    expect(sent.settings.set_signature.account).toBe('work');
    expect(sent.settings.set_signature.reply).toBe(false);
    const signature = sent.settings.set_signature.signature as {
      body_html: string;
      body_plain: string;
    };
    expect(signature.body_html).toContain('<strong>Ada</strong>');
    expect(signature.body_plain).toBe('Ada');
  });

  it('clears the slot through the clear control', async () => {
    render(<ComposingDetail />);
    await screen.findByLabelText('Message body');

    fireEvent.click(screen.getByRole('button', { name: 'Clear signature' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"settings":{"set_signature":{"account":"work","reply":false,"signature":null}}}',
    );
  });

  it('serves both slots from one editing pass, the current one included', async () => {
    render(<ComposingDetail />);
    await screen.findByLabelText('Message body');

    fireEvent.click(screen.getByRole('button', { name: 'Use for both' }));

    await waitFor(() => expect(ipc.dispatch).toHaveBeenCalledTimes(2));
    const slots = ipc.dispatch.mock.calls.map(
      (call) => (call[0] as { settings: { set_signature: { reply: boolean } } }).settings
        .set_signature,
    );
    expect(slots.map((slot) => slot.reply).sort()).toEqual([false, true]);
  });

  it('carries the persist failure sentence', () => {
    useSettingsStore.setState({ error: 'persist_failed' });
    render(<ComposingDetail />);

    expect(screen.getByRole('alert').textContent).toContain('could not be saved');
  });
});
