// SPDX-License-Identifier: GPL-3.0-only
// The account discovery step: one email asks the host for the server
// settings, the wizard renders the recommendation and its choices, the manual
// toggle pre-fills or defaults the form behind it, and a miss, a failure, or a
// bound-out answer falls back to the manual form with a visible note. A
// settlement from a closed session never lands on a reopened wizard.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
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

const untrustedImapView: RecommendationView = {
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
      is_trusted: false,
      source: 'http://autoconfig.example.test/mail/config-v1.1.xml',
    },
  },
  choices: [{ capability: 'Mail', on: true, server_found: true }],
  detected: { outcome: 'mail', source_kind: 'Autoconfig' },
};

const googleView: RecommendationView = {
  recommendation: { Google: { email: 'user@gmail.example' } },
  choices: [],
  detected: { outcome: 'mail', source_kind: null },
};

const microsoftView: RecommendationView = {
  recommendation: { Microsoft: { email: 'user@outlook.example' } },
  choices: [],
  detected: { outcome: 'mail', source_kind: null },
};

const jmapView: RecommendationView = {
  recommendation: {
    Jmap: {
      email: 'user@example.test',
      server_url: 'https://jmap.example.test/.well-known/jmap',
      is_trusted: true,
      source: 'https://jmap.example.test/.well-known/jmap',
    },
  },
  choices: [{ capability: 'Mail', on: true, server_found: true }],
  detected: { outcome: 'jmap', source_kind: 'JmapWellKnown' },
};

const davView: RecommendationView = {
  recommendation: {
    Dav: {
      email: 'user@example.test',
      caldav_url: 'https://cal.example.test/',
      carddav_url: 'https://card.example.test/',
    },
  },
  choices: [{ capability: 'Calendar', on: true, server_found: true }],
  detected: { outcome: 'dav', source_kind: null },
};

const manualView: RecommendationView = {
  recommendation: { Manual: { reason: 'NothingFound' } },
  choices: [],
  detected: { outcome: { nothing: { network_error: false } }, source_kind: null },
};

function manualMissView(reason: 'NetworkError' | 'OauthOnlyProvider'): RecommendationView {
  return {
    recommendation: { Manual: { reason } },
    choices: [],
    detected: {
      outcome: { nothing: { network_error: reason === 'NetworkError' } },
      source_kind: null,
    },
  };
}

/** Opens the wizard, submits one email, and waits for the host's answer. */
async function detectEmail(view: RecommendationView, email = 'user@example.test') {
  ipc.detectAccountSettings.mockResolvedValue(view);
  render(<AccountSetupFlow />);
  fireEvent.change(screen.getByLabelText('Email address'), { target: { value: email } });
  fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
  await screen.findByText('Recommended setup');
}

/** Types the one address and submits it, on whatever step the wizard shows. */
function submitEmail(email = 'user@example.test') {
  fireEvent.change(screen.getByLabelText('Email address'), { target: { value: email } });
  fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
}

beforeEach(() => {
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
  it('submits one email to the host and shows a loading state until it answers', async () => {
    let answer!: (view: RecommendationView) => void;
    ipc.detectAccountSettings.mockReturnValue(
      new Promise<RecommendationView>((resolve) => {
        answer = resolve;
      }),
    );
    render(<AccountSetupFlow />);
    submitEmail();

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
    render(<AccountSetupFlow />);
    submitEmail();

    expect(
      await screen.findByText('No settings were found for this address; enter the server by hand.'),
    ).toBeTruthy();
    expect(screen.getByLabelText('Mail server')).toHaveProperty('value', '');
    expect(screen.getByLabelText('Email address')).toHaveProperty('value', 'user@example.test');
  });

  it('falls back to the manual form with a note when detection fails', async () => {
    ipc.detectAccountSettings.mockRejectedValue('the lookup failed: no route to host');
    render(<AccountSetupFlow />);
    submitEmail();

    expect(
      await screen.findByText(
        'Detection failed: the lookup failed: no route to host; enter the server by hand.',
      ),
    ).toBeTruthy();
    expect(screen.getByLabelText('Mail server')).toHaveProperty('value', '');
  });

  it('falls back to the manual form when the client bound times the detect out', async () => {
    vi.useFakeTimers();
    ipc.detectAccountSettings.mockReturnValue(new Promise<RecommendationView>(() => {}));
    render(<AccountSetupFlow />);
    submitEmail();
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

  it('a detection that settles after close leaves the reopened wizard on the email step', async () => {
    let answer!: (view: RecommendationView) => void;
    ipc.detectAccountSettings.mockReturnValue(
      new Promise<RecommendationView>((resolve) => {
        answer = resolve;
      }),
    );
    render(<AccountSetupFlow />);
    submitEmail();
    expect(screen.getByRole('button', { name: 'Looking up the settings…' })).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(useUiStore.getState().accountSetupOpen).toBe(false);
    await act(async () => {
      answer(imapView);
    });

    await act(async () => {
      useUiStore.setState({ accountSetupOpen: true });
    });
    const reopen = screen.getByRole('button', { name: 'Continue' });
    expect(reopen).toHaveProperty('disabled', false);
    expect(screen.queryByText('Recommended setup')).toBeNull();
    expect(screen.queryByText(/enter the server by hand/)).toBeNull();
  });

  it("a stale settlement cannot clear a fresh detection's busy state", async () => {
    const answers: Array<(view: RecommendationView) => void> = [];
    ipc.detectAccountSettings.mockImplementation(
      () => new Promise<RecommendationView>((resolve) => answers.push(resolve)),
    );
    render(<AccountSetupFlow />);

    submitEmail();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await act(async () => {
      useUiStore.setState({ accountSetupOpen: true });
    });

    submitEmail();
    expect(screen.getByRole('button', { name: 'Looking up the settings…' })).toBeTruthy();

    await act(async () => {
      answers[0]!(imapView);
    });
    expect(screen.getByRole('button', { name: 'Looking up the settings…' })).toBeTruthy();
    expect(screen.queryByText('Recommended setup')).toBeNull();

    await act(async () => {
      answers[1]!(googleView);
    });
    expect(await screen.findByText('Google account')).toBeTruthy();
  });

  it('notes a network miss', async () => {
    ipc.detectAccountSettings.mockResolvedValue(manualMissView('NetworkError'));
    render(<AccountSetupFlow />);
    submitEmail();

    expect(
      await screen.findByText('Detection could not reach the network; enter the server by hand.'),
    ).toBeTruthy();
    expect(screen.getByLabelText('Mail server')).toBeTruthy();
  });

  it('notes an oauth-only provider miss', async () => {
    ipc.detectAccountSettings.mockResolvedValue(manualMissView('OauthOnlyProvider'));
    render(<AccountSetupFlow />);
    submitEmail();

    expect(
      await screen.findByText(
        'This provider only offers browser sign-in, which this build does not offer yet; enter the server by hand.',
      ),
    ).toBeTruthy();
    expect(screen.getByLabelText('Mail server')).toBeTruthy();
  });

  it('summarises a jmap route', async () => {
    await detectEmail(jmapView);

    expect(screen.getByText('JMAP')).toBeTruthy();
    expect(screen.getByText('https://jmap.example.test/.well-known/jmap')).toBeTruthy();
  });

  it('summarises a microsoft route', async () => {
    await detectEmail(microsoftView);

    expect(screen.getByText('Microsoft account')).toBeTruthy();
  });

  it('summarises a dav route', async () => {
    await detectEmail(davView);

    expect(screen.getByText('Calendar and contacts')).toBeTruthy();
    expect(screen.getByLabelText('Calendar')).toHaveProperty('checked', true);
  });
});

describe('the untrusted-approval gate', () => {
  it('discloses an untrusted imap recommendation and withholds the form until it is approved', async () => {
    await detectEmail(untrustedImapView);

    expect(
      screen.getByText(
        'These settings were discovered over an unencrypted connection and must be approved before use.',
      ),
    ).toBeTruthy();
    expect(screen.getByLabelText('I trust these settings')).toHaveProperty('checked', false);

    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));

    expect(screen.getByText('Recommended setup')).toBeTruthy();
    expect(screen.queryByLabelText('Mail server')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Test connection' })).toBeNull();
  });

  it('an approved untrusted recommendation hands the discovered host onto the form', async () => {
    await detectEmail(untrustedImapView);
    fireEvent.click(screen.getByLabelText('I trust these settings'));
    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));

    expect(screen.getByLabelText('Mail server')).toHaveProperty('value', 'imap.example.test');
    expect(screen.getByLabelText('Port')).toHaveProperty('value', '993');
    expect(screen.getByRole('button', { name: 'Test connection' })).toBeTruthy();
  });

  it('a trusted recommendation shows no disclosure and continues immediately', async () => {
    await detectEmail(imapView);

    expect(screen.queryByText(/discovered over an unencrypted connection/)).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Set up manually' }));

    expect(screen.getByLabelText('Mail server')).toHaveProperty('value', 'imap.example.test');
  });
});
