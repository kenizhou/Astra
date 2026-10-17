// SPDX-License-Identifier: GPL-3.0-only
// The wizard's JMAP form: one secret field labelled "Password or API
// token" (the server declares its own scheme in the 401 and the engine
// negotiates, so whether the person pasted a password or an API token
// changes nothing this form needs to ask), the server URL editable
// over the derived display, and, only where the host's advertise probe
// says the server offers discoverable OAuth, a sign-in button beside
// it. The probe fails soft: any miss leaves the field standing, never
// an error, because the password form works against every server.

import { useEffect, useRef, useState, type FormEvent } from 'react';

import {
  addJmapAccount,
  dispatch,
  intents,
  jmapOauthBegin,
  jmapOauthOffered,
  oauthCancel,
  onJmapSignInResult,
  type JmapSignInOutcome,
} from '@/services/ipc';
import { isTauri } from '@/events/surfaceEvents';

/** The JMAP sign-in state machine: idle, browser-open, then the result. */
type JmapFlowState =
  | { status: 'idle'; error?: string }
  | { status: 'browser-open' }
  | { status: 'result'; outcome: JmapSignInOutcome };

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

export interface JmapFormProps {
  /** The seed the flow routed here on: the address and the server URL. */
  seed: { email: string; serverUrl: string };
  /** Closes the wizard; the form calls it on a successful add. */
  onClose: () => void;
}

export function JmapForm({ seed, onClose }: JmapFormProps) {
  const [email] = useState(seed.email);
  // A blank seed URL shows as the domain-derived `https://<domain>`: the
  // display the host re-derives authoritatively at the submit.
  const [url, setUrl] = useState(() => {
    if (seed.serverUrl !== '') {
      return seed.serverUrl;
    }
    const at = seed.email.indexOf('@');
    return at >= 0 ? `https://${seed.email.slice(at + 1)}` : seed.serverUrl;
  });
  const [password, setPassword] = useState('');
  const [offered, setOffered] = useState(false);
  const [flow, setFlow] = useState<JmapFlowState>({ status: 'idle' });
  // The unmount cleanup below reads the LIVE status; a closure over the
  // initial state would see 'idle' forever.
  const flowRef = useRef(flow);
  flowRef.current = flow;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The advertise probe runs once per form on the seed's server, the
  // recommendation the person is looking at: a server that advertises
  // nothing, or a probe that could not ask, offers nothing and never
  // fails the form.
  useEffect(() => {
    let live = true;
    jmapOauthOffered(seed.email, seed.serverUrl).then(
      (answered) => {
        if (live) {
          setOffered(answered);
        }
      },
      () => {
        /* the probe's miss is the offer's no */
      },
    );
    return () => {
      live = false;
    };
  }, [seed.email, seed.serverUrl]);

  // The completion subscription stands while the form lives, so it is
  // registered before any begin can return.
  useEffect(() => {
    if (!isTauri) {
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | null = null;
    onJmapSignInResult((result) => {
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
      onClose();
    }
  }, [flow, onClose]);

  const onSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await addJmapAccount(email.trim(), url.trim(), password);
      await dispatch(intents.refreshMail());
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const onSignIn = () => {
    setFlow({ status: 'browser-open' });
    jmapOauthBegin(email.trim(), url.trim()).catch((err: unknown) => {
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

  const refused =
    flow.status === 'result' && 'refused' in flow.outcome ? flow.outcome.refused : null;
  // A refused completion brings the buttons back, because both
  // consent-asking refusals are remedied by beginning again; an added
  // one closes the wizard above.
  const showOffer =
    offered && (flow.status === 'idle' || (flow.status === 'result' && refused !== null));

  return (
    <form onSubmit={onSubmit} className="grid gap-3">
      <label className="grid gap-1 text-xs text-muted-foreground">
        Email address
        <input
          value={email}
          onChange={() => {
            /* the address the flow began with names the account */
          }}
          readOnly
          spellCheck={false}
          className="rounded-full border border-input bg-background px-3 py-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
        />
      </label>
      <label className="grid gap-1 text-xs text-muted-foreground">
        Server URL
        <input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          autoComplete="off"
          spellCheck={false}
          placeholder="https://mail.example.net"
          className="rounded-full border border-input bg-background px-3 py-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
        />
      </label>
      <label className="grid gap-1 text-xs text-muted-foreground">
        Password or API token
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="new-password"
          className="rounded-full border border-input bg-background px-3 py-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
        />
      </label>
      <p className="-mt-2 text-[11px] text-muted-foreground">
        The one secret goes to the system keychain, never to a log file. A password, an app
        password, or an API token all work: the server says which scheme it wants.
      </p>

      {showOffer && (
        <div className="grid gap-2 rounded-[var(--radius-xs)] border border-border p-3">
          <p className="text-xs text-muted-foreground">Or sign in with this server</p>
          <div>
            <button
              type="button"
              onClick={onSignIn}
              className="rounded-[var(--radius-xs)] bg-secondary px-3 py-1.5 text-[13px] text-secondary-foreground hover:bg-hover"
            >
              Sign in with your provider
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

      {refused !== null && (
        <div className="grid gap-1 rounded-[var(--radius-xs)] border border-border p-3">
          <p role="status" className="text-xs text-error">
            {refusalNote(refused.refusal)}
          </p>
          <p className="text-xs text-muted-foreground">{refused.detail}</p>
        </div>
      )}

      {error !== null && (
        <p role="alert" className="text-xs text-error">
          {error}
        </p>
      )}

      <div className="mt-1 flex items-center justify-end gap-2">
        <button
          type="submit"
          disabled={busy}
          className="rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-40 focus-visible:outline-none transition-colors focus-visible:border-primary"
        >
          Add account
        </button>
      </div>
    </form>
  );
}
