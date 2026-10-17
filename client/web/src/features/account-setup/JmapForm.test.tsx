// SPDX-License-Identifier: GPL-3.0-only
// The wizard's JMAP form: one field labelled "Password or API token"
// (never two secret fields), the derived server URL editable, the
// password submit building the document host-side, and the OAuth offer
// rendered only where the host's advertise probe said the server offers
// discoverable sign-in (fail soft: a refused or never-answering probe
// leaves the field standing, never an error).
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: the form registers its result listener only
  // inside Tauri, so the jsdom test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  addJmapAccount: vi.fn(),
  jmapOauthOffered: vi.fn(),
  jmapOauthBegin: vi.fn(),
  oauthCancel: vi.fn(),
  onJmapSignInResult: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import type { JmapSignInResult } from '@/services/ipc';
import { JmapForm } from './JmapForm';
import { useUiStore } from '@/stores/uiStore';

/** The handlers the form registered for the result event. */
const resultHandlers: Array<(result: JmapSignInResult) => void> = [];

/** Delivers one completion event to the form, as the host would. */
function deliver(result: JmapSignInResult) {
  act(() => {
    for (const handler of resultHandlers) {
      handler(result);
    }
  });
}

/** Renders the form as the flow does, on the seed a recommendation carries. */
function renderForm(seed: { email: string; serverUrl: string }) {
  render(<JmapForm seed={seed} onClose={() => useUiStore.getState().setAccountSetupOpen(false)} />);
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.addJmapAccount.mockReset();
  ipc.addJmapAccount.mockResolvedValue({
    account_id: 'user@example.test@jmap:mail.example.test',
    email: 'user@example.test',
  });
  ipc.jmapOauthOffered.mockReset();
  ipc.jmapOauthOffered.mockResolvedValue(false);
  ipc.jmapOauthBegin.mockReset();
  ipc.jmapOauthBegin.mockResolvedValue(undefined);
  ipc.oauthCancel.mockReset();
  ipc.oauthCancel.mockResolvedValue(undefined);
  ipc.onJmapSignInResult.mockReset();
  resultHandlers.length = 0;
  ipc.onJmapSignInResult.mockImplementation(
    async (handler: (result: JmapSignInResult) => void) => {
      resultHandlers.push(handler);
      return () => {
        /* the listener teardown */
      };
    },
  );
  useUiStore.setState({ accountSetupOpen: true });
});

afterEach(() => {
  cleanup();
  useUiStore.setState({ accountSetupOpen: false });
});

describe('the jmap form', () => {
  it('asks for one secret field labelled Password or API token, never two', () => {
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });

    const secret = screen.getByLabelText('Password or API token');
    expect(secret).toHaveProperty('type', 'password');
    expect(
      screen.queryByLabelText(/^password$/i),
      'the bare Password label belongs to the IMAP form alone'
    ).toBeNull();
  });

  it('shows the server URL editable, derived from the email domain when the seed carries none', () => {
    renderForm({ email: 'user@example.test', serverUrl: '' });

    const url = screen.getByLabelText('Server URL');
    expect(url).toHaveProperty('value', 'https://example.test');
    fireEvent.change(url, { target: { value: 'https://mail.example.net' } });
    expect(screen.getByLabelText('Server URL')).toHaveProperty('value', 'https://mail.example.net');
  });

  it('submits the fields to the host, which derives the document server-side', async () => {
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });
    fireEvent.change(screen.getByLabelText('Password or API token'), {
      target: { value: 'an-api-token' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'Add account' }));
    await waitFor(() => expect(ipc.addJmapAccount).toHaveBeenCalledTimes(1));
    expect(ipc.addJmapAccount).toHaveBeenCalledWith(
      'user@example.test',
      'https://mail.example.test',
      'an-api-token',
    );
    expect(ipc.dispatch).toHaveBeenCalled();
    await waitFor(() => expect(useUiStore.getState().accountSetupOpen).toBe(false));
  });

  it('surfaces the host add failure beside the field without closing', async () => {
    ipc.addJmapAccount.mockRejectedValue('the account could not be connected: sign-in rejected');
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });
    fireEvent.change(screen.getByLabelText('Password or API token'), {
      target: { value: 'wrong' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'Add account' }));

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toMatch(/sign-in rejected/i);
    expect(useUiStore.getState().accountSetupOpen).toBe(true);
  });
});

describe('the advertise-gated oauth offer', () => {
  it('renders the sign-in button only when the host says the server advertises it', async () => {
    ipc.jmapOauthOffered.mockResolvedValue(true);
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });

    expect(
      await screen.findByRole('button', { name: 'Sign in with your provider' })
    ).toBeTruthy();
    expect(ipc.jmapOauthOffered).toHaveBeenCalledWith(
      'user@example.test',
      'https://mail.example.test',
    );
  });

  it('renders no button when the server advertises nothing', async () => {
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });

    await waitFor(() => expect(ipc.jmapOauthOffered).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole('button', { name: 'Sign in with your provider' })).toBeNull();
    expect(screen.getByLabelText('Password or API token')).toBeTruthy();
  });

  it('fails soft: a refused probe offers nothing and raises no error', async () => {
    ipc.jmapOauthOffered.mockRejectedValue('the probe could not run');
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });

    await waitFor(() => expect(ipc.jmapOauthOffered).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole('button', { name: 'Sign in with your provider' })).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByLabelText('Password or API token')).toBeTruthy();
  });
});

describe('a begun jmap sign-in', () => {
  it('begins with the address and the server url and shows the browser-open state', async () => {
    ipc.jmapOauthOffered.mockResolvedValue(true);
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });

    fireEvent.click(await screen.findByRole('button', { name: 'Sign in with your provider' }));
    await waitFor(() => expect(ipc.jmapOauthBegin).toHaveBeenCalledTimes(1));
    expect(ipc.jmapOauthBegin).toHaveBeenCalledWith(
      'user@example.test',
      'https://mail.example.test',
    );
    expect(screen.getByRole('status').textContent).toMatch(/browser is open/);
    expect(screen.getByRole('button', { name: 'Cancel sign-in' })).toBeTruthy();
  });

  it('a refused begin surfaces its sentence and brings the buttons back', async () => {
    ipc.jmapOauthOffered.mockResolvedValue(true);
    ipc.jmapOauthBegin.mockRejectedValue(
      "this server's sign-in could not be discovered: no authorization server metadata"
    );
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });

    fireEvent.click(await screen.findByRole('button', { name: 'Sign in with your provider' }));

    expect(await screen.findByRole('alert')).toBeTruthy();
    expect(
      await screen.findByRole('button', { name: 'Sign in with your provider' })
    ).toBeTruthy();
  });

  it('the cancel returns to the password field', async () => {
    ipc.jmapOauthOffered.mockResolvedValue(true);
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });

    fireEvent.click(await screen.findByRole('button', { name: 'Sign in with your provider' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Cancel sign-in' }));

    expect(ipc.oauthCancel).toHaveBeenCalledTimes(1);
    expect(
      await screen.findByRole('button', { name: 'Sign in with your provider' })
    ).toBeTruthy();
  });

  it('a completed sign-in closes the wizard on the added account', async () => {
    ipc.jmapOauthOffered.mockResolvedValue(true);
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });
    fireEvent.click(await screen.findByRole('button', { name: 'Sign in with your provider' }));
    await waitFor(() => expect(ipc.jmapOauthBegin).toHaveBeenCalledTimes(1));

    deliver({ outcome: { added: { address: 'user@example.test' } } });

    await waitFor(() => expect(useUiStore.getState().accountSetupOpen).toBe(false));
    expect(ipc.dispatch).toHaveBeenCalled();
  });

  it('a refused completion shows the typed sentence beside the buttons', async () => {
    ipc.jmapOauthOffered.mockResolvedValue(true);
    renderForm({ email: 'user@example.test', serverUrl: 'https://mail.example.test' });
    fireEvent.click(await screen.findByRole('button', { name: 'Sign in with your provider' }));
    await waitFor(() => expect(ipc.jmapOauthBegin).toHaveBeenCalledTimes(1));

    deliver({
      outcome: {
        refused: { refusal: 'dead', detail: 'the refresh token was refused' },
      },
    });

    expect(
      await screen.findByText(/reports this sign-in expired or withdrawn/)
    ).toBeTruthy();
    expect(
      screen.getByRole('button', { name: 'Sign in with your provider' })
    ).toBeTruthy();
    expect(useUiStore.getState().accountSetupOpen).toBe(true);
  });
});
