// SPDX-License-Identifier: GPL-3.0-only
// The manual setup form behind the wizard's manual toggle: a failed test
// connection surfaces the message and stops, a certificate rejection offers
// the fingerprint for an accepted retry, a passing test carries through
// add_account and the first refresh, and the connection security crosses to
// the test command. The discovery step's tests live in AccountDiscovery.test.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  mailboxList: vi.fn(),
  syncProgress: vi.fn(),
  addAccount: vi.fn(),
  imapTestConnection: vi.fn(),
  detectAccountSettings: vi.fn(),
  oauthOffer: vi.fn(),
  imapAuthOffer: vi.fn(),
  settings: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { AccountSetupFlow } from './AccountSetupFlow';
import { useUiStore } from '@/stores/uiStore';
import { useSettingsStore } from '@/features/settings/settingsStore';
import type { SettingsSnapshot } from '@/services/ipc';

const rejectedCertificate = {
  server_name: 'mail.example.net',
  sha256: 'AB:CD:EF:01',
  subject_common_name: 'mail.example.net',
  subject_organisation: null,
  issuer_common_name: 'mail.example.net',
  issuer_organisation: null,
  not_before: null,
  not_after: null,
};

/** The settings pull the ask step reads, holding the added account's row
 *  in whichever ask state a test needs. */
function settingsSnapshot(needsName: boolean): SettingsSnapshot {
  return {
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
        address: 'work@example.net',
        kind: 'imap_smtp',
        legs: { calendar: false, contacts: false },
        needs_sender_name: needsName,
      },
    ],
    about: { version: '0.1.0' },
  };
}

async function fillForm() {
  fireEvent.change(screen.getByLabelText('Mail server'), {
    target: { value: 'imap.example.net' },
  });
  fireEvent.change(screen.getByLabelText('Port'), { target: { value: '993' } });
  fireEvent.change(screen.getByLabelText('Email address'), {
    target: { value: 'work@example.net' },
  });
  // The auth card draws nothing credential-shaped until its ask lands:
  // wait for the ask itself, then type the secret only where the landed
  // answer stands a password field (the sign-in answer demotes it).
  await waitFor(
    () => expect(ipc.imapAuthOffer).toHaveBeenCalled(),
    { timeout: 3000 },
  );
  await waitFor(() => {
    const password = screen.queryByLabelText('Password');
    if (password !== null) {
      fireEvent.change(password, { target: { value: 'secret' } });
    }
  }, { timeout: 3000 });
}

beforeEach(() => {
  ipc.imapTestConnection.mockReset();
  ipc.addAccount.mockReset();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.detectAccountSettings.mockReset();
  ipc.oauthOffer.mockReset();
  ipc.oauthOffer.mockResolvedValue({ providers: [] });
  ipc.imapAuthOffer.mockReset();
  ipc.imapAuthOffer.mockResolvedValue('password');
  ipc.settings.mockReset();
  ipc.settings.mockResolvedValue(settingsSnapshot(true));
  useSettingsStore.setState({ accounts: [], revision: 0 });
  useUiStore.setState({ accountSetupOpen: true });
});

afterEach(() => {
  cleanup();
  useUiStore.setState({ accountSetupOpen: false });
});

describe('the manual setup flow', () => {
  it('renders nothing while closed', () => {
    useUiStore.setState({ accountSetupOpen: false });
    const { container } = render(<AccountSetupFlow />);
    expect(container.childElementCount).toBe(0);
  });

  it('closes on Escape and moves the focus in on open', () => {
    render(<AccountSetupFlow />);

    const dialog = screen.getByRole('dialog');
    expect(dialog.contains(document.activeElement)).toBe(true);

    fireEvent.keyDown(dialog, { key: 'Escape' });
    expect(useUiStore.getState().accountSetupOpen).toBe(false);
  });

  it('assures the password goes to the system keychain', async () => {
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));

    // The card draws nothing credential-shaped until its ask lands, and
    // the ask runs once the server and address stand: fill them, let the
    // mocked password answer land, and the assurance rides it.
    fireEvent.change(screen.getByLabelText('Mail server'), {
      target: { value: 'imap.example.net' },
    });
    fireEvent.change(screen.getByLabelText('Email address'), {
      target: { value: 'work@example.net' },
    });
    expect(
      await screen.findByText(
        'Your password goes to the system keychain, never to a log file.',
        {},
        { timeout: 3000 },
      ),
    ).toBeTruthy();
  });

  it('closes on a click on the backdrop, but not on one inside the dialog', () => {
    const { container } = render(<AccountSetupFlow />);
    const backdrop = container.firstChild as HTMLElement;

    fireEvent.click(screen.getByRole('dialog'));
    expect(useUiStore.getState().accountSetupOpen).toBe(true);

    fireEvent.click(backdrop);
    expect(useUiStore.getState().accountSetupOpen).toBe(false);
  });

  it('a failed test connection surfaces the message and never adds the account', async () => {
    ipc.imapTestConnection.mockResolvedValue({
      failed: { message: 'the server refused the sign in' },
    });
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
    await fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));

    expect(await screen.findByText('the server refused the sign in')).toBeTruthy();
    expect(ipc.imapTestConnection).toHaveBeenCalledTimes(1);
    expect(ipc.addAccount).not.toHaveBeenCalled();
  });

  it('a passing test carries through add account, the first refresh, and the name ask', async () => {
    ipc.imapTestConnection.mockResolvedValue({ ok: { account_id: 'work' } });
    ipc.addAccount.mockResolvedValue({ account_id: 'work', email: 'work@example.net' });
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
    await fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));

    const add = await screen.findByRole('button', { name: 'Add account' });
    fireEvent.click(add);

    // The ask-once step follows the join: the wizard stays open and asks
    // the name the account's mail goes out under.
    const nameField = await screen.findByLabelText(/what name/i);
    expect(ipc.addAccount).toHaveBeenCalledTimes(1);
    const toml = ipc.addAccount.mock.calls[0]![0] as string;
    expect(toml).toContain('addr = "imap.example.net:993"');
    expect(toml).toContain('server_name = "imap.example.net"');
    expect(toml).toContain('username = "work@example.net"');
    expect(toml).toContain('password = "secret"');
    expect(toml).not.toContain('certificate_exception');
    expect(ipc.dispatch).toHaveBeenCalledWith('refresh_mail');

    fireEvent.change(nameField, { target: { value: 'Grace Hopper' } });
    fireEvent.click(screen.getByRole('button', { name: /continue/i }));
    expect(ipc.dispatch).toHaveBeenCalledWith({
      settings: { set_sender_name: { account: 'work', name: 'Grace Hopper' } },
    });
    await waitFor(() => expect(useUiStore.getState().accountSetupOpen).toBe(false));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('the name ask never draws where the join adopted a name', async () => {
    // The adoption rides the spawned sync: where the provider already
    // answered a name, the account joins with the ask spent, and the
    // step closes the wizard without asking anything.
    ipc.settings.mockResolvedValue(settingsSnapshot(false));
    ipc.imapTestConnection.mockResolvedValue({ ok: { account_id: 'work' } });
    ipc.addAccount.mockResolvedValue({ account_id: 'work', email: 'work@example.net' });
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
    await fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Add account' }));

    await waitFor(() => expect(useUiStore.getState().accountSetupOpen).toBe(false));
    expect(screen.queryByLabelText(/name/i)).toBeNull();
    expect(
      ipc.dispatch.mock.calls.some((call) => {
        const intent = call[0] as { settings?: { set_sender_name?: unknown } };
        return intent?.settings?.set_sender_name !== undefined;
      }),
      'an adopted name spends the ask: nothing sets a sender name'
    ).toBe(false);
  });

  it('a late adoption closes the name ask cleanly and never spends it twice', async () => {
    // The ask reads the CURRENT needs state when the step renders and
    // again when it settles: a provider-known name can arrive mid-step
    // (the adoption rides the spawned sync), and the step must close on
    // it rather than overwrite the adopted name with a typed one.
    const { act } = await import('@testing-library/react');
    ipc.imapTestConnection.mockResolvedValue({ ok: { account_id: 'work' } });
    ipc.addAccount.mockResolvedValue({ account_id: 'work', email: 'work@example.net' });
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
    await fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Add account' }));

    await screen.findByLabelText(/name/i);

    // The adoption lands mid-step, the way the settings republish carries
    // it: the step reads the current needs state and closes.
    act(() => {
      useSettingsStore.getState().applySnapshot(settingsSnapshot(false));
    });

    await waitFor(() => expect(useUiStore.getState().accountSetupOpen).toBe(false));
    expect(
      ipc.dispatch.mock.calls.some((call) => {
        const intent = call[0] as { settings?: { set_sender_name?: unknown } };
        return intent?.settings?.set_sender_name !== undefined;
      }),
      'the late adoption spent the ask: the step never writes a name over it'
    ).toBe(false);
  });

  it('shows the fingerprint of a rejected certificate and accepts it on retry', async () => {
    // The retry's add joins a provider that already answered a name, so
    // the ask-once step never draws and the wizard closes on the add.
    ipc.settings.mockResolvedValue(settingsSnapshot(false));
    ipc.imapTestConnection.mockResolvedValueOnce({
      rejected_certificate: {
        certificate: rejectedCertificate,
        reason: 'the certificate is self signed',
      },
    });
    ipc.imapTestConnection.mockResolvedValueOnce({ ok: { account_id: 'work' } });
    ipc.addAccount.mockResolvedValue({ account_id: 'work', email: 'work@example.net' });
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
    await fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));

    // The Server row and the Claims to be row both carry the name.
    expect(await screen.findAllByText('mail.example.net')).toHaveLength(2);
    expect(screen.getByText('AB:CD:EF:01')).toBeTruthy();
    expect(
      screen.getByText('the certificate did not verify: the certificate is self signed'),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Accept and continue' }));

    await waitFor(() => expect(ipc.imapTestConnection).toHaveBeenCalledTimes(2));
    const retrySetup = ipc.imapTestConnection.mock.calls[1]![0];
    expect(retrySetup.accepted_certificate).toEqual({
      server_name: 'mail.example.net',
      sha256: 'AB:CD:EF:01',
    });

    fireEvent.click(await screen.findByRole('button', { name: 'Add account' }));
    await waitFor(() => expect(useUiStore.getState().accountSetupOpen).toBe(false));
    const toml = ipc.addAccount.mock.calls[0]![0] as string;
    expect(toml).toContain('[[certificate_exception]]');
    expect(toml).toContain('server_name = "mail.example.net"');
    expect(toml).toContain('sha256 = "abcdef01"');
    expect(ipc.dispatch).toHaveBeenCalledWith('refresh_mail');
  });

  it('surfaces a typed error when add account itself fails', async () => {
    ipc.imapTestConnection.mockResolvedValue({ ok: { account_id: 'work' } });
    ipc.addAccount.mockRejectedValue('the account could not be connected: no route to host');
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
    await fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Add account' }));

    expect(await screen.findByText(/no route to host/)).toBeTruthy();
    expect(useUiStore.getState().accountSetupOpen).toBe(true);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('asks the dialled server and stands the sign-in button primary beside the demoted password', async () => {
    // The manual step's own ask: the flow hands the card its fields, the
    // server's sign-in answer stands the primary button, and the password
    // demotes behind the control rather than standing beside it.
    ipc.imapAuthOffer.mockResolvedValue({
      sign_in: {
        issuer: 'https://id.example.test',
        provider_label: null,
        password_also_works: true,
      },
    });
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
    await fillForm();

    expect(
      await screen.findByRole('button', { name: 'Sign in with id.example.test' })
    ).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Use a password instead' })).toBeTruthy();
    expect(screen.queryByLabelText('Password')).toBeNull();
    expect(ipc.imapAuthOffer).toHaveBeenCalledTimes(1);
  });

  it('carries the start_tls security through the tested setup', async () => {
    ipc.imapTestConnection.mockResolvedValue({ ok: { account_id: 'work' } });
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
    await fillForm();
    fireEvent.change(screen.getByLabelText('Connection security'), {
      target: { value: 'start_tls' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));

    await screen.findByText('The connection works. Ready to add.');
    const setup = ipc.imapTestConnection.mock.calls[0]![0];
    expect(setup.imap_security).toBe('start_tls');
    expect(setup.imap_host).toBe('imap.example.net:993');
  });
});
