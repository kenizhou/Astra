// SPDX-License-Identifier: GPL-3.0-only
// The wizard's IMAP step's auth card: the ask runs once the host and
// the address stand, bounded and fail-soft in both directions (a
// refused or never-answering question is the password screen, never an
// error), and the answer draws one of three screens. The sign-in answer
// stands the sign-in button primary with the issuer's host named and
// demotes the password behind a secondary control; the password is
// never removed, and is hidden entirely only where the server itself
// admits no password. The registration answer carries the
// only-pre-registered-apps sentence with the password standing per its
// flag. The card is inert until the answer lands.

import { useEffect, useRef, useState } from 'react';

import {
  dispatch,
  imapAuthOffer,
  imapOauthBegin,
  intents,
  oauthCancel,
  onImapSignInResult,
  type AccountSetup,
  type ConnectionSecurity,
  type ImapAuthAnswer,
  type ImapSignInOutcome,
  type RejectedCertificateView,
} from '@/services/ipc';
import { isTauri } from '@/events/surfaceEvents';
import { buildSetup } from './detection';

/** How long the card waits for the fields to settle before it asks: the
 * person types the host before the address, and a probe per keystroke
 * is a question the server never needs asked. */
const ASK_SETTLE_MS = 400;

/** The client bound around the ask, mirroring the host's own ten second
 * word: the probe and issuer budgets are inside it, and the belt and
 * braces here never leaves the card waiting on a lost answer. */
const ASK_BOUND_MS = 10_000;

/** The card's own flow state machine: idle, browser-open, then the
 * result. */
type ImapFlowState =
  | { status: 'idle'; error?: string }
  | { status: 'browser-open' }
  | { status: 'result'; outcome: ImapSignInOutcome };

/** The ask under the client bound: the first of the host's answer or
 * the timeout wins, and the timer never outlives the race. */
function imapAuthWithGuard(
  setup: AccountSetup,
  oauthIssuer: string | undefined,
): Promise<ImapAuthAnswer> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const guard = new Promise<ImapAuthAnswer>((resolve) => {
    timer = setTimeout(() => resolve('password'), ASK_BOUND_MS);
  });
  return Promise.race([imapAuthOffer(setup, oauthIssuer), guard]).finally(() =>
    clearTimeout(timer),
  );
}

/** The sentence one refusal classification carries. */
function refusalNote(refusal: string): string {
  switch (refusal) {
    case 'dead':
      return 'The server reports this sign-in expired or withdrawn. Sign in again; the consent screen will reappear.';
    case 'underscoped':
      return 'The consent given misses a permission this build needs. Sign in again and consent to the full list.';
    default:
      return 'The sign-in could not be completed; reaching the server failed. The password field still works.';
  }
}

/** The name the sign-in button carries: the provider's own label when
 * this build holds a registration, else the issuer's host. */
export function issuerLabel(issuer: string, providerLabel: string | null): string {
  if (providerLabel !== null) {
    return providerLabel;
  }
  try {
    return new URL(issuer).host;
  } catch {
    return issuer;
  }
}

export interface ImapAuthCardProps {
  /** The IMAP server host the person typed. */
  host: string;
  /** The port the person typed, blank for the security's default. */
  port: string;
  /** How the IMAP connection is secured. */
  security: ConnectionSecurity;
  /** The account's email address. */
  username: string;
  /** The issuer the provider's own autoconfig named, when detection carried one. */
  oauthIssuer?: string;
  /** The certificate the person accepted, when one was. */
  accepted: RejectedCertificateView | null;
  /** The typed password, however the answer gates the field. */
  password: string;
  /** Hands a password edit back to the flow. */
  onPasswordChange: (value: string) => void;
  /** A completed sign-in added the account: the wizard closes on it. */
  onAdded: () => void;
}

export function ImapAuthCard({
  host,
  port,
  security,
  username,
  oauthIssuer,
  accepted,
  password,
  onPasswordChange,
  onAdded,
}: ImapAuthCardProps) {
  // The ask's state: idle before the fields stand, asking while the
  // card waits, answered with one of the three screens.
  const [ask, setAsk] = useState<'idle' | 'asking' | { answer: ImapAuthAnswer }>('idle');
  // The last answer that landed, held through re-asks so the card never
  // rebuilds a screen the person may be typing into: nothing
  // credential-shaped renders until the first answer lands, and an edit's
  // re-ask keeps the standing screen while it asks again.
  const [landed, setLanded] = useState<ImapAuthAnswer | null>(null);
  const [flow, setFlow] = useState<ImapFlowState>({ status: 'idle' });
  const [passwordOpen, setPasswordOpen] = useState(false);
  // The unmount cleanup reads the LIVE status; a closure over the
  // initial state would see 'idle' forever.
  const flowRef = useRef(flow);
  flowRef.current = flow;

  // The ask runs once the host and the address stand, re-asking whenever
  // the fields it depends on change. It fails soft in both directions:
  // a refused or never-answering question is the password screen.
  useEffect(() => {
    if (host.trim() === '' || username.trim() === '') {
      setAsk('idle');
      setLanded(null);
      return;
    }
    let live = true;
    setAsk('asking');
    const timer = setTimeout(() => {
      const setup = buildSetup(
        { host, port, username, password: '', security },
        accepted ?? undefined,
      );
      imapAuthWithGuard(setup, oauthIssuer).then(
        (answer) => {
          if (live) {
            setAsk({ answer });
            setLanded(answer);
          }
        },
        () => {
          if (live) {
            const answer: ImapAuthAnswer = 'password';
            setAsk({ answer });
            setLanded(answer);
          }
        },
      );
    }, ASK_SETTLE_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // The password never gates the question: the host asks without one.
  }, [host, port, security, username, oauthIssuer, accepted]);

  // The completion subscription stands while the card lives, so it is
  // registered before any begin can return.
  useEffect(() => {
    if (!isTauri) {
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | null = null;
    onImapSignInResult((result) => {
      setFlow((standing) =>
        standing.status === 'browser-open'
          ? { status: 'result', outcome: result.outcome }
          : standing,
      );
    }).then(
      (teardown) => {
        if (disposed) {
          teardown();
        } else {
          unlisten = teardown;
        }
      },
      () => {
        /* a listener that cannot register hears nothing */
      },
    );
    return () => {
      disposed = true;
      unlisten?.();
      unlisten = null;
      // The wizard closing abandons a browser-open flow: the shared
      // sign-in slot is taken back so a later begin (any provider) does
      // not answer InFlight into the void.
      if (flowRef.current.status === 'browser-open' && isTauri) {
        void oauthCancel();
      }
    };
  }, []);

  // A completed sign-in added the account: the first sync runs in the
  // background and the wizard closes on it.
  useEffect(() => {
    if (flow.status === 'result' && 'added' in flow.outcome) {
      void dispatch(intents.refreshMail());
      onAdded();
    }
  }, [flow, onAdded]);

  // The screen derives from the last LANDED answer, never the ask's
  // transient state: while the first ask stands, nothing
  // credential-shaped renders; while a re-ask stands, the last screen
  // holds so no field appears and is then taken away.
  const answer = landed;
  const signIn =
    answer !== null && typeof answer === 'object' && 'sign_in' in answer
      ? answer.sign_in
      : null;
  const registrationNeeded =
    answer !== null && typeof answer === 'object' && 'registration_needed' in answer
      ? answer.registration_needed
      : null;
  const showOffer =
    signIn !== null &&
    (flow.status === 'idle' || (flow.status === 'result' && 'refused' in flow.outcome));

  const onSignIn = () => {
    const setup = buildSetup(
      { host, port, username, password: '', security },
      accepted ?? undefined,
    );
    setFlow({ status: 'browser-open' });
    imapOauthBegin(setup, oauthIssuer).catch((err: unknown) => {
      const sentence = err instanceof Error ? err.message : String(err);
      setFlow((standing) =>
        standing.status === 'browser-open' ? { status: 'idle', error: sentence } : standing,
      );
    });
  };

  const onCancel = () => {
    void oauthCancel();
    setFlow({ status: 'idle' });
  };

  // The password area's three standings: nothing at all before the first
  // answer lands (a field that appears and is then taken away reads as
  // the app changing its mind), hidden entirely where the server admits
  // no password, demoted behind a secondary control where the sign-in
  // stands and a password still works, and standing as today everywhere
  // else. Never removed while the screen lives.
  const screenLanded = landed !== null;
  const passwordHidden =
    !screenLanded ||
    (signIn !== null && !signIn.password_also_works) ||
    (registrationNeeded !== null && !registrationNeeded.password_also_works);
  const passwordDemoted = screenLanded && signIn !== null && signIn.password_also_works;
  const passwordStanding =
    screenLanded && !passwordHidden && !(passwordDemoted && !passwordOpen);

  return (
    <>
      {ask === 'asking' && (
        <div
          role="status"
          aria-label="Asking the server"
          className="grid gap-1 rounded-[var(--radius-xs)] border border-border p-3"
        >
          <p className="text-xs text-muted-foreground">
            Asking {host.trim()} what it accepts...
          </p>
        </div>
      )}

      {registrationNeeded !== null && (
        <div
          role="status"
          aria-label="Sign-in needs registration"
          className="grid gap-1 rounded-[var(--radius-xs)] border border-border p-3"
        >
          <p className="text-xs text-muted-foreground">
            {host.trim()} takes a provider sign-in, but only from applications its
            provider registered in advance; this build holds no registration for it.
          </p>
          <p className="text-xs text-muted-foreground">
            {registrationNeeded.password_also_works
              ? 'Use your password or an app password below.'
              : 'Ask its provider for a way in; this server admits no password here.'}
          </p>
        </div>
      )}

      {showOffer && signIn !== null && (
        <div className="grid gap-2 rounded-[var(--radius-xs)] border border-border p-3">
          <div>
            <button
              type="button"
              onClick={onSignIn}
              className="rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 focus-visible:outline-none transition-colors"
            >
              Sign in with {issuerLabel(signIn.issuer, signIn.provider_label)}
            </button>
          </div>
          {flow.status === 'idle' && flow.error !== undefined && (
            <p role="alert" className="text-xs text-error">
              {flow.error}
            </p>
          )}
        </div>
      )}

      {flow.status === 'browser-open' && (
        <div className="grid gap-2 rounded-[var(--radius-xs)] border border-border p-3">
          <p role="status" className="text-xs text-muted-foreground">
            The browser is open; finish signing in there.
          </p>
          <div>
            <button
              type="button"
              onClick={onCancel}
              className="rounded-[var(--radius-xs)] bg-secondary px-3 py-1.5 text-[13px] text-secondary-foreground hover:bg-hover"
            >
              Cancel sign-in
            </button>
          </div>
        </div>
      )}

      {flow.status === 'result' && 'refused' in flow.outcome && (
        <div className="grid gap-1 rounded-[var(--radius-xs)] border border-border p-3">
          <p role="status" className="text-xs text-error">
            {refusalNote(flow.outcome.refused.refusal)}
          </p>
          <p className="text-xs text-muted-foreground">{flow.outcome.refused.detail}</p>
        </div>
      )}

      {passwordDemoted && !passwordOpen && (
        <>
          <p className="text-xs text-muted-foreground">or, if you prefer:</p>
          <div>
            <button
              type="button"
              onClick={() => setPasswordOpen(true)}
              className="rounded-[var(--radius-xs)] bg-secondary px-3 py-1.5 text-[13px] text-secondary-foreground hover:bg-hover focus-visible:outline-none transition-colors"
            >
              Use a password instead
            </button>
          </div>
        </>
      )}
      {passwordStanding && (
        <>
          <label className="grid gap-1 text-xs text-muted-foreground">
            Password
            <input
              type="password"
              value={password}
              onChange={(e) => onPasswordChange(e.target.value)}
              autoComplete="new-password"
              className="rounded-full border border-input bg-background px-3 py-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
            />
          </label>
          <p className="-mt-2 text-[11px] text-muted-foreground">
            Your password goes to the system keychain, never to a log file.
          </p>
        </>
      )}
    </>
  );
}
