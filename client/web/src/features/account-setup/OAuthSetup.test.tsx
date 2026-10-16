// SPDX-License-Identifier: GPL-3.0-only
// The wizard's OAuth offer: the provider sign-in buttons render only for
// providers this build carries a registration for (the host answers none
// today, so the production render stays hidden and the tests seed the
// seam), a begun sign-in shows the browser-open state with its cancel,
// the completion event lands as the success sentence or the typed
// refusal sentence, and a late result for a flow the wizard did not
// begin changes nothing.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: the flow registers its result listener only
  // inside Tauri, so the jsdom test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  mailboxList: vi.fn(),
  syncProgress: vi.fn(),
  addAccount: vi.fn(),
  imapTestConnection: vi.fn(),
  detectAccountSettings: vi.fn(),
  oauthOffer: vi.fn(),
  oauthBegin: vi.fn(),
  oauthCancel: vi.fn(),
  onOauthSignInResult: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import type { OAuthSignInResult } from '@/services/ipc';
import { AccountSetupFlow } from './AccountSetupFlow';
import { useUiStore } from '@/stores/uiStore';

/** The handlers the flow registered for the result event. */
const resultHandlers: Array<(result: OAuthSignInResult) => void> = [];

/** Delivers one completion event to the flow, as the host would. */
function deliver(result: OAuthSignInResult) {
  act(() => {
    for (const handler of resultHandlers) {
      handler(result);
    }
  });
}

/** Opens the wizard and lands on the manual form the offer sits beside. */
async function openOnManualForm(email = 'work@example.test') {
  render(<AccountSetupFlow />);
  fireEvent.change(screen.getByLabelText('Email address'), { target: { value: email } });
  fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
  await screen.findByLabelText('Mail server');
}

beforeEach(() => {
  ipc.detectAccountSettings.mockReset();
  ipc.addAccount.mockReset();
  ipc.dispatch.mockReset();
  ipc.imapTestConnection.mockReset();
  ipc.oauthOffer.mockReset();
  ipc.oauthOffer.mockResolvedValue({ providers: [] });
  ipc.oauthBegin.mockReset();
  ipc.oauthBegin.mockResolvedValue({ provider: 'microsoft' });
  ipc.oauthCancel.mockReset();
  ipc.oauthCancel.mockResolvedValue(undefined);
  ipc.onOauthSignInResult.mockReset();
  resultHandlers.length = 0;
  ipc.onOauthSignInResult.mockImplementation(
    async (handler: (result: OAuthSignInResult) => void) => {
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

describe('the oauth offer', () => {
  it('renders nothing when the build offers no provider', async () => {
    await openOnManualForm();
    expect(screen.queryByText('Sign in with Microsoft')).toBeNull();
    expect(screen.queryByText('Sign in with Google')).toBeNull();
    expect(ipc.oauthOffer).toHaveBeenCalledTimes(1);
  });

  it('renders one button per offered provider beside the password form', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft', 'google'] });
    await openOnManualForm();

    expect(await screen.findByText('Sign in with Microsoft')).toBeTruthy();
    expect(screen.getByText('Sign in with Google')).toBeTruthy();
    expect(screen.getByLabelText('Password')).toBeTruthy();
  });
});

describe('a begun sign-in', () => {
  it('begins with the provider and the address as the login hint', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft'] });
    await openOnManualForm('work@example.test');

    fireEvent.click(await screen.findByText('Sign in with Microsoft'));
    await waitFor(() => expect(ipc.oauthBegin).toHaveBeenCalledTimes(1));
    expect(ipc.oauthBegin).toHaveBeenCalledWith('microsoft', 'work@example.test');
    expect(
      screen.getByText('The browser is open; finish signing in with Microsoft there.'),
    ).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Cancel sign-in' })).toBeTruthy();
  });

  it('surfaces a begin that failed back on the buttons', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft'] });
    ipc.oauthBegin.mockRejectedValue('the system browser could not be opened: no browser');
    await openOnManualForm();

    fireEvent.click(await screen.findByText('Sign in with Microsoft'));
    expect(
      await screen.findByText('the system browser could not be opened: no browser'),
    ).toBeTruthy();
    expect(screen.getByText('Sign in with Microsoft')).toBeTruthy();
  });

  it('cancels and comes home to the buttons', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft'] });
    await openOnManualForm();
    fireEvent.click(await screen.findByText('Sign in with Microsoft'));

    fireEvent.click(await screen.findByRole('button', { name: 'Cancel sign-in' }));
    await waitFor(() => expect(ipc.oauthCancel).toHaveBeenCalledTimes(1));
    expect(screen.getByText('Sign in with Microsoft')).toBeTruthy();
    expect(
      screen.queryByText('The browser is open; finish signing in with Microsoft there.'),
    ).toBeNull();
  });

  it('cancels a standing flow when the wizard closes', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft'] });
    await openOnManualForm();
    fireEvent.click(await screen.findByText('Sign in with Microsoft'));

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(useUiStore.getState().accountSetupOpen).toBe(false));
    expect(ipc.oauthCancel).toHaveBeenCalledTimes(1);
  });
});

describe('the completion event', () => {
  it('lands the honest success sentence', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft'] });
    await openOnManualForm();
    fireEvent.click(await screen.findByText('Sign in with Microsoft'));
    await waitFor(() => expect(ipc.oauthBegin).toHaveBeenCalledTimes(1));

    deliver({ provider: 'microsoft', outcome: 'signed_in' });

    expect(
      screen.getByText(
        'Signed in with Microsoft. This build cannot add a Microsoft account yet; the account type arrives with the next update.',
      ),
    ).toBeTruthy();
    expect(screen.queryByText('Sign in with Microsoft')).toBeNull();
  });

  it('lands the added account sentence with its address', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft'] });
    await openOnManualForm();
    fireEvent.click(await screen.findByText('Sign in with Microsoft'));
    await waitFor(() => expect(ipc.oauthBegin).toHaveBeenCalledTimes(1));

    deliver({
      provider: 'microsoft',
      outcome: { added: { address: 'alice@example.com', calendar_withheld: false } },
    });

    expect(
      screen.getByText(
        'Signed in with Microsoft as alice@example.com. The account joins with its first sync in the background.',
      ),
    ).toBeTruthy();
    expect(screen.queryByText('Sign in with Microsoft')).toBeNull();
  });

  it('lands the added account sentence for a google sign-in', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['google'] });
    await openOnManualForm();
    fireEvent.click(await screen.findByText('Sign in with Google'));
    await waitFor(() => expect(ipc.oauthBegin).toHaveBeenCalledTimes(1));

    deliver({
      provider: 'google',
      outcome: { added: { address: 'alice@gmail.example', calendar_withheld: false } },
    });

    expect(
      screen.getByText(
        'Signed in with Google as alice@gmail.example. The account joins with its first sync in the background.',
      ),
    ).toBeTruthy();
    expect(screen.queryByText('Sign in with Google')).toBeNull();
  });

  it('names a withheld calendar in the added sentence', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft'] });
    await openOnManualForm();
    fireEvent.click(await screen.findByText('Sign in with Microsoft'));
    await waitFor(() => expect(ipc.oauthBegin).toHaveBeenCalledTimes(1));

    deliver({
      provider: 'microsoft',
      outcome: { added: { address: 'alice@example.com', calendar_withheld: true } },
    });

    expect(
      screen.getByText(
        'Signed in with Microsoft as alice@example.com. The account joins with its first sync in the background. ' +
          'The calendar is missing from this sign-in; sign in again and consent to the calendar permission.',
      ),
    ).toBeTruthy();
  });

  it('asks for consent again on a dead grant, with the detail beside', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft'] });
    await openOnManualForm();
    fireEvent.click(await screen.findByText('Sign in with Microsoft'));
    await waitFor(() => expect(ipc.oauthBegin).toHaveBeenCalledTimes(1));

    deliver({
      provider: 'microsoft',
      outcome: {
        refused: { refusal: 'dead', detail: 'oauth endpoint error: invalid_grant' },
      },
    });

    expect(
      screen.getByText(
        'Microsoft reports this sign-in expired or withdrawn. Sign in again; the consent screen will reappear.',
      ),
    ).toBeTruthy();
    expect(screen.getByText('oauth endpoint error: invalid_grant')).toBeTruthy();
    expect(screen.getByText('Sign in with Microsoft')).toBeTruthy();
  });

  it('asks for the wider consent on an underscoped grant', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['google'] });
    await openOnManualForm();
    fireEvent.click(await screen.findByText('Sign in with Google'));
    await waitFor(() => expect(ipc.oauthBegin).toHaveBeenCalledTimes(1));

    deliver({
      provider: 'google',
      outcome: {
        refused: { refusal: 'underscoped', detail: 'oauth endpoint error: invalid_scope' },
      },
    });

    expect(
      screen.getByText(
        'The consent given misses a permission this build needs. Sign in again and consent to the full list.',
      ),
    ).toBeTruthy();
    expect(screen.getByText('Sign in with Google')).toBeTruthy();
  });

  it('says transport trouble and changes nothing on an indeterminate refusal', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft'] });
    await openOnManualForm();
    fireEvent.click(await screen.findByText('Sign in with Microsoft'));
    await waitFor(() => expect(ipc.oauthBegin).toHaveBeenCalledTimes(1));

    deliver({
      provider: 'microsoft',
      outcome: {
        refused: { refusal: 'indeterminate', detail: 'oauth transport: connection refused' },
      },
    });

    expect(
      screen.getByText(
        'The sign-in could not be completed; reaching the provider failed. Nothing changed about any sign-in that stands.',
      ),
    ).toBeTruthy();
    expect(screen.getByText('Sign in with Microsoft')).toBeTruthy();
  });

  it('ignores a result for a flow the wizard did not begin', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['microsoft'] });
    await openOnManualForm();
    fireEvent.click(await screen.findByText('Sign in with Microsoft'));
    await waitFor(() => expect(ipc.oauthBegin).toHaveBeenCalledTimes(1));

    deliver({ provider: 'google', outcome: 'signed_in' });
    expect(
      screen.getByText('The browser is open; finish signing in with Microsoft there.'),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Cancel sign-in' }));
    deliver({ provider: 'microsoft', outcome: 'signed_in' });
    expect(
      screen.getByText('Sign in with Microsoft'),
    ).toBeTruthy();
    expect(
      screen.queryByText(
        'Signed in with Microsoft. This build cannot add a Microsoft account yet; the account type arrives with the next update.',
      ),
    ).toBeNull();
  });
});

describe('the oauth-only miss note', () => {
  it('names the provider sign-in when the build offers one', async () => {
    ipc.oauthOffer.mockResolvedValue({ providers: ['google'] });
    ipc.detectAccountSettings.mockResolvedValue({
      recommendation: { Manual: { reason: 'OauthOnlyProvider' } },
      choices: [],
      detected: { outcome: { nothing: { network_error: false } }, source_kind: null },
    });
    render(<AccountSetupFlow />);
    fireEvent.change(screen.getByLabelText('Email address'), {
      target: { value: 'user@gmail.example' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }));

    expect(
      await screen.findByText(
        'This provider only offers browser sign-in; sign in with the provider, or enter the server by hand.',
      ),
    ).toBeTruthy();
  });
});
