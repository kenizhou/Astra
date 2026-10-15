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
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { AccountSetupFlow } from './AccountSetupFlow';
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
  ipc.oauthOffer.mockReset();
  ipc.oauthOffer.mockResolvedValue({ providers: [] });
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
    fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));

    expect(await screen.findByText('the server refused the sign in')).toBeTruthy();
    expect(ipc.imapTestConnection).toHaveBeenCalledTimes(1);
    expect(ipc.addAccount).not.toHaveBeenCalled();
  });

  it('a passing test carries through add account and the first refresh, then closes', async () => {
    ipc.imapTestConnection.mockResolvedValue({ ok: { account_id: 'work' } });
    ipc.addAccount.mockResolvedValue({ account_id: 'work', email: 'work@example.net' });
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
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
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
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
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
    fillForm();
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Add account' }));

    expect(await screen.findByText(/no route to host/)).toBeTruthy();
    expect(useUiStore.getState().accountSetupOpen).toBe(true);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('carries the start_tls security through the tested setup', async () => {
    ipc.imapTestConnection.mockResolvedValue({ ok: { account_id: 'work' } });
    render(<AccountSetupFlow />);
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));
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
