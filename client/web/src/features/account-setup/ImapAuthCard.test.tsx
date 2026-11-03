// SPDX-License-Identifier: GPL-3.0-only
// The wizard's IMAP step's auth card: the ask is bounded and fail-soft
// in both directions (a refused or never-answering question is the
// password screen), the sign-in answer renders the primary button with
// the issuer's host named and demotes the password behind a secondary
// control (never removes it; hides it entirely only where the server
// admits no password), the registration answer carries the
// only-pre-registered-apps sentence with the password standing per its
// flag, and a completed sign-in closes the wizard on the added account.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: the card registers its result listener only
  // inside Tauri, so the jsdom test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  imapAuthOffer: vi.fn(),
  imapOauthBegin: vi.fn(),
  oauthCancel: vi.fn(),
  onImapSignInResult: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import type { ImapAuthAnswer, ImapSignInResult } from '@/services/ipc';
import { ImapAuthCard } from './ImapAuthCard';

/** The handlers the card registered for the result event. */
const resultHandlers: Array<(result: ImapSignInResult) => void> = [];

/** Delivers one completion event to the card, as the host would. */
function deliver(result: ImapSignInResult) {
  act(() => {
    for (const handler of resultHandlers) {
      handler(result);
    }
  });
}

/** The step's fields, ready for the question. */
const fields = {
  host: 'mail.example.test',
  port: '993',
  security: 'implicit_tls' as const,
  username: 'ada@example.test',
};

/** The setup payload the card builds from the ready fields. */
function setupPayload() {
  return {
    imap_host: 'mail.example.test:993',
    username: 'ada@example.test',
    credential: { password: '' },
    imap_security: 'implicit_tls',
    smtp_security: 'implicit_tls',
  };
}

/** Renders the card as the manual step does. */
function renderCard(over: Partial<Parameters<typeof ImapAuthCard>[0]> = {}) {
  const onAdded = vi.fn();
  const view = render(
    <ImapAuthCard
      host={fields.host}
      port={fields.port}
      security={fields.security}
      username={fields.username}
      accepted={null}
      password=""
      onPasswordChange={() => {
        /* the flow's own setter */
      }}
      onAdded={onAdded}
      {...over}
    />,
  );
  return { onAdded, view };
}

/** Waits past the ask's settle so a mocked answer lands. */
async function answerArrives() {
  await act(async () => {
    vi.advanceTimersByTime(600);
  });
}

/** Flushes the click's promise and React's re-render under fake timers,
 * where waitFor's own polling timer would never advance. */
async function flush() {
  await act(async () => {
    vi.advanceTimersByTime(10);
  });
}

const SIGN_IN: ImapAuthAnswer = {
  sign_in: {
    issuer: 'https://id.example.test',
    provider_label: null,
    password_also_works: true,
  },
};

beforeEach(() => {
  vi.useFakeTimers();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.imapAuthOffer.mockReset();
  ipc.imapAuthOffer.mockResolvedValue('password');
  ipc.imapOauthBegin.mockReset();
  ipc.imapOauthBegin.mockResolvedValue(undefined);
  ipc.oauthCancel.mockReset();
  ipc.oauthCancel.mockResolvedValue(undefined);
  ipc.onImapSignInResult.mockReset();
  resultHandlers.length = 0;
  ipc.onImapSignInResult.mockImplementation(
    async (handler: (result: ImapSignInResult) => void) => {
      resultHandlers.push(handler);
      return () => {
        /* the listener teardown */
      };
    },
  );
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('the ask', () => {
  it('asks once the host and address stand, over the step\'s own payload', async () => {
    renderCard();
    expect(ipc.imapAuthOffer).not.toHaveBeenCalled();
    await answerArrives();
    expect(ipc.imapAuthOffer).toHaveBeenCalledTimes(1);
    expect(ipc.imapAuthOffer).toHaveBeenCalledWith(setupPayload(), undefined);
  });

  it('stays inert while the server is asked', async () => {
    let never: ((answer: ImapAuthAnswer) => void) | undefined;
    ipc.imapAuthOffer.mockReturnValue(
      new Promise<ImapAuthAnswer>((resolve) => {
        never = resolve;
      }),
    );
    renderCard();
    await answerArrives();

    const card = screen.getByRole('status', { name: 'Asking the server' });
    expect(card.textContent).toMatch(/Asking mail.example.test/);
    expect(
      screen.queryByRole('button', { name: /Sign in with/ }),
      'no button renders before the answer lands'
    ).toBeNull();
    expect(
      screen.queryByLabelText('Password'),
      'nothing credential-shaped renders before the answer lands'
    ).toBeNull();
    never?.('password');
  });

  it('a refused or never-answering question is the password screen', async () => {
    ipc.imapAuthOffer.mockRejectedValue('the ask could not run');
    renderCard();
    await answerArrives();

    expect(screen.getByLabelText('Password')).toBeTruthy();
    expect(screen.queryByRole('status', { name: 'Asking the server' })).toBeNull();
  });

  it('asks nothing while the host or address stands empty', async () => {
    renderCard({ host: '', username: '' });
    await answerArrives();

    expect(ipc.imapAuthOffer).not.toHaveBeenCalled();
    expect(
      screen.queryByLabelText('Password'),
      'the card stays inert while it has nothing to ask'
    ).toBeNull();
  });
});

describe('the sign-in screen', () => {
  it('renders the primary button naming the issuer host and demotes the password', async () => {
    ipc.imapAuthOffer.mockResolvedValue(SIGN_IN);
    renderCard();
    await answerArrives();

    const button = screen.getByRole('button', { name: 'Sign in with id.example.test' });
    expect(button.className).toContain('bg-primary');
    expect(
      screen.queryByLabelText('Password'),
      'the password is demoted, not standing beside the primary action'
    ).toBeNull();
    expect(
      screen.getByRole('button', { name: 'Use a password instead' }),
      'the secondary control that reveals the demoted field renders'
    ).toBeTruthy();
  });

  it('the demoted password is never removed: the control reveals the field', async () => {
    ipc.imapAuthOffer.mockResolvedValue(SIGN_IN);
    const onPasswordChange = vi.fn();
    renderCard({ onPasswordChange });
    await answerArrives();

    fireEvent.click(screen.getByRole('button', { name: 'Use a password instead' }));

    const field = screen.getByLabelText('Password');
    expect(field).toBeTruthy();
    expect(
      screen.getByText('Your password goes to the system keychain, never to a log file.')
    ).toBeTruthy();
    fireEvent.change(field, { target: { value: 'secret' } });
    expect(onPasswordChange).toHaveBeenCalledWith('secret');
  });

  it('hides the password entirely where the server admits none', async () => {
    ipc.imapAuthOffer.mockResolvedValue({
      sign_in: {
        issuer: 'https://id.example.test',
        provider_label: null,
        password_also_works: false,
      },
    });
    renderCard();
    await answerArrives();

    expect(screen.getByRole('button', { name: 'Sign in with id.example.test' })).toBeTruthy();
    expect(screen.queryByLabelText('Password')).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'Use a password instead' }),
      'the control would be a dead end on a server that takes no password'
    ).toBeNull();
  });

  it('names the provider by its own label when the build holds a registration', async () => {
    ipc.imapAuthOffer.mockResolvedValue({
      sign_in: {
        issuer: 'https://id.example.net',
        provider_label: 'Example Mail',
        password_also_works: true,
      },
    });
    renderCard();
    await answerArrives();

    expect(screen.getByRole('button', { name: 'Sign in with Example Mail' })).toBeTruthy();
  });
});

describe('the registration screen', () => {
  it('carries the only-pre-registered-apps sentence with the password standing', async () => {
    ipc.imapAuthOffer.mockResolvedValue({
      registration_needed: { password_also_works: true },
    });
    renderCard();
    await answerArrives();

    const card = screen.getByRole('status', { name: 'Sign-in needs registration' });
    expect(card.textContent).toMatch(/registered in advance/);
    expect(card.textContent).toContain('mail.example.test');
    expect(screen.getByLabelText('Password')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Use a password instead' })).toBeNull();
  });

  it('hides the password where the registration server admits none', async () => {
    ipc.imapAuthOffer.mockResolvedValue({
      registration_needed: { password_also_works: false },
    });
    renderCard();
    await answerArrives();

    expect(screen.getByRole('status', { name: 'Sign-in needs registration' })).toBeTruthy();
    expect(screen.queryByLabelText('Password')).toBeNull();
  });
});

describe('the password screen', () => {
  it('renders the password form as today, with no card chrome', async () => {
    ipc.imapAuthOffer.mockResolvedValue('password');
    renderCard();
    await answerArrives();

    expect(screen.getByLabelText('Password')).toBeTruthy();
    expect(
      screen.getByText('Your password goes to the system keychain, never to a log file.')
    ).toBeTruthy();
    expect(screen.queryByRole('status')).toBeNull();
    expect(screen.queryByRole('button', { name: /Sign in with/ })).toBeNull();
  });
});

describe('a begun mail sign-in', () => {
  async function begun() {
    ipc.imapAuthOffer.mockResolvedValue(SIGN_IN);
    renderCard();
    await answerArrives();
    fireEvent.click(screen.getByRole('button', { name: 'Sign in with id.example.test' }));
    await flush();
    expect(ipc.imapOauthBegin).toHaveBeenCalledTimes(1);
  }

  it('begins over the step\'s payload and shows the browser-open state', async () => {
    await begun();

    expect(ipc.imapOauthBegin).toHaveBeenCalledWith(setupPayload(), undefined);
    expect(screen.getByRole('status').textContent).toMatch(/browser is open/);
    expect(screen.getByRole('button', { name: 'Cancel sign-in' })).toBeTruthy();
  });

  it('a refused begin surfaces its sentence and brings the button back', async () => {
    ipc.imapOauthBegin.mockRejectedValue(
      "this server's sign-in could not be discovered: no authorization server answered"
    );
    ipc.imapAuthOffer.mockResolvedValue(SIGN_IN);
    renderCard();
    await answerArrives();
    fireEvent.click(screen.getByRole('button', { name: 'Sign in with id.example.test' }));
    await flush();

    expect(screen.getByRole('alert').textContent).toMatch(/could not be discovered/);
    expect(
      screen.getByRole('button', { name: 'Sign in with id.example.test' })
    ).toBeTruthy();
  });

  it('the cancel returns to the sign-in button', async () => {
    await begun();

    fireEvent.click(screen.getByRole('button', { name: 'Cancel sign-in' }));
    await flush();

    expect(ipc.oauthCancel).toHaveBeenCalledTimes(1);
    expect(
      screen.getByRole('button', { name: 'Sign in with id.example.test' })
    ).toBeTruthy();
  });

  it('a completed sign-in dispatches the refresh and closes the wizard', async () => {
    const onAdded = vi.fn();
    ipc.imapAuthOffer.mockResolvedValue(SIGN_IN);
    renderCard({ onAdded });
    await answerArrives();
    fireEvent.click(screen.getByRole('button', { name: 'Sign in with id.example.test' }));
    await flush();
    expect(ipc.imapOauthBegin).toHaveBeenCalledTimes(1);

    deliver({ outcome: { added: { address: 'ada@example.test' } } });
    await flush();

    expect(onAdded).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith('refresh_mail');
  });

  it('a refused completion shows the typed sentence beside the button', async () => {
    await begun();

    deliver({
      outcome: {
        refused: { refusal: 'dead', detail: 'the refresh token was refused' },
      },
    });
    await flush();

    expect(
      screen.getByText(/reports this sign-in expired or withdrawn/)
    ).toBeTruthy();
    expect(
      screen.getByRole('button', { name: 'Sign in with id.example.test' })
    ).toBeTruthy();
  });

  it('cancels a browser-open sign-in when the card unmounts', async () => {
    await begun();
    cleanup();
    expect(ipc.oauthCancel).toHaveBeenCalledTimes(1);
  });
});

describe('the re-ask', () => {
  it('holds the landed screen while an edit asks again', async () => {
    ipc.imapAuthOffer.mockResolvedValue(SIGN_IN);
    const { view } = renderCard();
    await answerArrives();
    expect(screen.queryByLabelText('Password')).toBeNull();

    // The host edits: the ask re-runs, and the landed sign-in screen
    // holds rather than rebuilding a password field mid-ask.
    let never: ((answer: ImapAuthAnswer) => void) | undefined;
    ipc.imapAuthOffer.mockReturnValue(
      new Promise<ImapAuthAnswer>((resolve) => {
        never = resolve;
      }),
    );
    await act(async () => {
      view.rerender(
        <ImapAuthCard
          host="mail2.example.test"
          port={fields.port}
          security={fields.security}
          username={fields.username}
          accepted={null}
          password=""
          onPasswordChange={() => {
            /* the flow's own setter */
          }}
          onAdded={() => {
            /* the flow's own callback */
          }}
        />,
      );
    });
    await answerArrives();
    expect(screen.getByRole('status', { name: 'Asking the server' })).toBeTruthy();
    expect(
      screen.queryByLabelText('Password'),
      'a field on screen stays; the card does not rebuild mid-ask'
    ).toBeNull();
    expect(
      screen.getByRole('button', { name: /Sign in with/ }),
      'the landed sign-in screen holds through the re-ask'
    ).toBeTruthy();
    never?.('password');
  });
});
