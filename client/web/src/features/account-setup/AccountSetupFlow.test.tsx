// SPDX-License-Identifier: GPL-3.0-only
// The account setup flow: an email asks the host for the server settings and
// falls back to the manual form on a miss, a failed test connection surfaces
// the message and stops, a certificate rejection offers the fingerprint for an
// accepted retry, and a passing test carries through add_account and the first
// refresh.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { RecommendationView } from '@/services/ipc';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  mailboxList: vi.fn(),
  syncProgress: vi.fn(),
  addAccount: vi.fn(),
  imapTestConnection: vi.fn(),
  detectAccountSettings: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { AccountSetupFlow } from './AccountSetupFlow';
import { DETECT_BOUND_MS } from './detection';
import { useUiStore } from '@/stores/uiStore';

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

const imapView: RecommendationView = {
  recommendation: {
    Imap: {
      email: 'user@example.test',
      imap_host: 'imap.example.test',
      imap_security: 'implicit_tls',
      smtp_security: 'implicit_tls',
      incoming: {
        protocol: 'IMAP',
        hostname: 'imap.example.test',
        port: 993,
        security: 'SSL/TLS',
        username: 'user@example.test',
      },
      is_trusted: true,
      source: 'https://autoconfig.example.test/mail/config-v1.1.xml',
    },
  },
  choices: [
    { capability: 'Mail', on: true, server_found: true },
    { capability: 'Calendar', on: false, server_found: false },
    { capability: 'Contacts', on: false, server_found: false },
  ],
  detected: { outcome: 'mail', source_kind: 'Autoconfig' },
};

const startTlsView: RecommendationView = {
  recommendation: {
    Imap: {
      email: 'user@example.test',
      imap_host: 'imap.example.test',
      imap_security: 'start_tls',
      smtp_security: 'start_tls',
      incoming: {
        protocol: 'IMAP',
        hostname: 'imap.example.test',
        port: 143,
        security: 'StartTLS',
        username: 'user@example.test',
      },
      is_trusted: true,
      source: 'srv',
    },
  },
  choices: [{ capability: 'Mail', on: true, server_found: true }],
  detected: { outcome: 'mail', source_kind: 'ImapSrv' },
};

const googleView: RecommendationView = {
  recommendation: { Google: { email: 'user@gmail.example' } },
  choices: [],
  detected: { outcome: 'mail', source_kind: null },
};

const manualView: RecommendationView = {
  recommendation: { Manual: { reason: 'NothingFound' } },
  choices: [],
  detected: { outcome: { nothing: { network_error: false } }, source_kind: null },
};

/** Opens the wizard, submits one email, and waits for the host's answer. */
async function detectEmail(view: RecommendationView, email = 'user@example.test') {
  ipc.detectAccountSettings.mockResolvedValue(view);
  render(<AccountSetupFlow />);
  fireEvent.change(screen.getByLabelText('Email address'), { target: { value: email } });
  fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
  await screen.findByText('Recommended setup');
}

function fillForm() {
  fireEvent.change(screen.getByLabelText('Mail server'), {
    target: { value: 'imap.example.net' },
  });
  fireEvent.change(screen.getByLabelText('Port'), { target: { value: '993' } });
  fireEvent.change(screen.getByLabelText('Email address'), {
    target: { value: 'work@example.net' },
  });
  fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'secret' } });
}

beforeEach(() => {
  ipc.imapTestConnection.mockReset();
  ipc.addAccount.mockReset();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.detectAccountSettings.mockReset();
  ipc.detectAccountSettings.mockResolvedValue(manualView);
  useUiStore.setState({ accountSetupOpen: true });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  useUiStore.setState({ accountSetupOpen: false });
});

describe('the account discovery step', () => {
  it('renders nothing while closed', () => {
    useUiStore.setState({ accountSetupOpen: false });
    const { container } = render(<AccountSetupFlow />);
    expect(container.childElementCount).toBe(0);
  });

  it('submits one email to the host and shows a loading state until it answers', async () => {
    let answer!: (view: RecommendationView) => void;
    ipc.detectAccountSettings.mockReturnValue(
      new Promise<RecommendationView>((resolve) => {
        answer = resolve;
      }),
    );
    render(<AccountSetupFlow />);
    fireEvent.change(screen.getByLabelText('Email address'), {
      target: { value: 'user@example.test' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }));

    const busy = screen.getByRole('button', { name: 'Looking up the settings…' });
    expect(busy).toHaveProperty('disabled', true);
    expect(ipc.detectAccountSettings).toHaveBeenCalledTimes(1);
    expect(ipc.detectAccountSettings).toHaveBeenCalledWith('user@example.test');

    await act(async () => {
      answer(imapView);
    });
    expect(await screen.findByText('Recommended setup')).toBeTruthy();
  });

  it('renders the detected servers and the choice toggles', async () => {
    await detectEmail(imapView);

    expect(screen.getByText('imap.example.test')).toBeTruthy();
    expect(screen.getByText('993')).toBeTruthy();
    expect(screen.getByText('SSL/TLS')).toBeTruthy();
    expect(screen.getByLabelText('Mail')).toHaveProperty('checked', true);
    expect(screen.getByLabelText('Calendar')).toHaveProperty('checked', false);
    expect(screen.getAllByText('not found')).toHaveLength(2);

    fireEvent.click(screen.getByLabelText('Calendar'));
    expect(screen.getByLabelText('Calendar')).toHaveProperty('checked', true);
  });

  it('opens the manual form prefilled with a found imap recommendation', async () => {
    await detectEmail(imapView);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));

    expect(screen.getByLabelText('Mail server')).toHaveProperty('value', 'imap.example.test');
    expect(screen.getByLabelText('Port')).toHaveProperty('value', '993');
    expect(screen.getByLabelText('Email address')).toHaveProperty('value', 'user@example.test');
    expect(screen.getByLabelText('Connection security')).toHaveProperty('value', 'implicit_tls');
    expect(screen.queryByText('Recommended setup')).toBeNull();
  });

  it('maps a start_tls recommendation onto the form it prefills', async () => {
    await detectEmail(startTlsView);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));

    expect(screen.getByLabelText('Port')).toHaveProperty('value', '143');
    expect(screen.getByLabelText('Connection security')).toHaveProperty('value', 'start_tls');
  });

  it('keeps manual defaults behind a non-imap recommendation', async () => {
    await detectEmail(googleView, 'user@gmail.example');
    expect(screen.getByText('Google account')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));

    expect(screen.getByLabelText('Mail server')).toHaveProperty('value', '');
    expect(screen.getByLabelText('Port')).toHaveProperty('value', '993');
    expect(screen.getByLabelText('Email address')).toHaveProperty('value', 'user@gmail.example');
    expect(screen.getByLabelText('Connection security')).toHaveProperty('value', 'implicit_tls');
  });

  it('notes a manual recommendation and opens the form', async () => {
    ipc.detectAccountSettings.mockResolvedValue(manualView);
    render(<AccountSetupFlow />);
    fireEvent.change(screen.getByLabelText('Email address'), {
      target: { value: 'user@example.test' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }));

    expect(
      await screen.findByText('No settings were found for this address; enter the server by hand.'),
    ).toBeTruthy();
    expect(screen.getByLabelText('Mail server')).toHaveProperty('value', '');
    expect(screen.getByLabelText('Email address')).toHaveProperty('value', 'user@example.test');
  });

  it('falls back to the manual form with a note when detection fails', async () => {
    ipc.detectAccountSettings.mockRejectedValue('the lookup failed: no route to host');
    render(<AccountSetupFlow />);
    fireEvent.change(screen.getByLabelText('Email address'), {
      target: { value: 'user@example.test' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }));

    expect(
      await screen.findByText('Detection failed: the lookup failed: no route to host; enter the server by hand.'),
    ).toBeTruthy();
    expect(screen.getByLabelText('Mail server')).toHaveProperty('value', '');
  });

  it('falls back to the manual form when the client bound times the detect out', async () => {
    vi.useFakeTimers();
    ipc.detectAccountSettings.mockReturnValue(new Promise<RecommendationView>(() => {}));
    render(<AccountSetupFlow />);
    fireEvent.change(screen.getByLabelText('Email address'), {
      target: { value: 'user@example.test' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    expect(screen.getByRole('button', { name: 'Looking up the settings…' })).toBeTruthy();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(DETECT_BOUND_MS);
    });

    expect(screen.getByLabelText('Mail server')).toBeTruthy();
    expect(
      screen.getByText('Detection did not answer within five seconds; enter the server by hand.'),
    ).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Looking up the settings…' })).toBeNull();
  });
});

describe('the manual setup flow', () => {
  beforeEach(() => {
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
  });

  it('a failed test connection surfaces the message and never adds the account', async () => {
    ipc.imapTestConnection.mockResolvedValue({
      failed: { message: 'the server refused the sign in' },
    });
    fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));

    expect(await screen.findByText('the server refused the sign in')).toBeTruthy();
    expect(ipc.imapTestConnection).toHaveBeenCalledTimes(1);
    expect(ipc.addAccount).not.toHaveBeenCalled();
  });

  it('a passing test carries through add account and the first refresh, then closes', async () => {
    ipc.imapTestConnection.mockResolvedValue({ ok: { account_id: 'work' } });
    ipc.addAccount.mockResolvedValue({ account_id: 'work', email: 'work@example.net' });
    fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));

    const add = await screen.findByRole('button', { name: 'Add account' });
    fireEvent.click(add);

    await waitFor(() => expect(useUiStore.getState().accountSetupOpen).toBe(false));
    expect(ipc.addAccount).toHaveBeenCalledTimes(1);
    const toml = ipc.addAccount.mock.calls[0]![0] as string;
    expect(toml).toContain('addr = "imap.example.net:993"');
    expect(toml).toContain('server_name = "imap.example.net"');
    expect(toml).toContain('username = "work@example.net"');
    expect(toml).toContain('password = "secret"');
    expect(toml).not.toContain('certificate_exception');
    expect(ipc.dispatch).toHaveBeenCalledWith('refresh_mail');
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('shows the fingerprint of a rejected certificate and accepts it on retry', async () => {
    ipc.imapTestConnection.mockResolvedValueOnce({
      rejected_certificate: {
        certificate: rejectedCertificate,
        reason: 'the certificate is self signed',
      },
    });
    ipc.imapTestConnection.mockResolvedValueOnce({ ok: { account_id: 'work' } });
    ipc.addAccount.mockResolvedValue({ account_id: 'work', email: 'work@example.net' });
    fillForm();
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
    fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Add account' }));

    expect(await screen.findByText(/no route to host/)).toBeTruthy();
    expect(useUiStore.getState().accountSetupOpen).toBe(true);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('carries the start_tls security through the tested setup', async () => {
    ipc.imapTestConnection.mockResolvedValue({ ok: { account_id: 'work' } });
    fillForm();
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
