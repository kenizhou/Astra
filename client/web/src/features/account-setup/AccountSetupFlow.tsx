// SPDX-License-Identifier: GPL-3.0-only
// The account setup flow: look up the server settings for one email address,
// then collect the mail server and its sign in, test the connection before
// anything is stored, offer a rejected certificate's fingerprint for a
// one-server exception, then add the account and start its first sync.
// Nothing is persisted until the test passes, and detection failing only ever
// lands on the manual form. Where the build carries a provider
// registration, the provider sign-in sits beside the password form.

import { useEffect, useRef, useState, type FormEvent } from 'react';

import {
  addAccount,
  dispatch,
  imapTestConnection,
  intents,
  oauthBegin,
  oauthCancel,
  oauthOffer,
  onOauthSignInResult,
  type AccountSetup,
  type OAuthProvider,
  type RecommendationView,
  type RejectedCertificateView,
} from '@/services/ipc';
import { isTauri } from '@/events/surfaceEvents';
import { useUiStore } from '@/stores/uiStore';
import { useDialogFocus } from '@/components/ui/useDialogFocus';
import { buildAccountToml } from './accountToml';
import { DetectStep } from './DetectStep';
import { CertificateOffer } from './CertificateOffer';
import { OAuthOffer, type OAuthFlowState } from './OAuthOffer';
import { RecommendationPanel } from './RecommendationPanel';
import {
  detectFallbackNote,
  detectWithGuard,
  EMPTY_FIELDS,
  manualDefaults,
  manualPrefill,
  missNote,
  recommendationEmail,
  type MissReason,
  type SetupFields,
} from './detection';

/** The wizard's steps: the email lookup, its confirmation, then the form. */
type SetupStep = 'detect' | 'recommendation' | 'manual';

/** The setup the test command takes, built from the fields on screen. */
function buildSetup(fields: SetupFields, accepted?: RejectedCertificateView): AccountSetup {
  const port = fields.port.trim();
  return {
    imap_host: port === '' ? fields.host.trim() : `${fields.host.trim()}:${port}`,
    username: fields.username.trim(),
    credential: { password: fields.password },
    imap_security: fields.security,
    smtp_security: fields.security,
    accepted_certificate: accepted
      ? { server_name: accepted.server_name, sha256: accepted.sha256 }
      : undefined,
  };
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function AccountSetupFlow() {
  const open = useUiStore((s) => s.accountSetupOpen);
  const detectEpoch = useRef(0);
  const [step, setStep] = useState<SetupStep>('detect');
  const [detectBusy, setDetectBusy] = useState(false);
  const [view, setView] = useState<RecommendationView | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [fields, setFields] = useState<SetupFields>(EMPTY_FIELDS);
  const [tested, setTested] = useState<string | null>(null);
  const [certificate, setCertificate] = useState<RejectedCertificateView | null>(null);
  const [accepted, setAccepted] = useState<RejectedCertificateView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [miss, setMiss] = useState<MissReason | null>(null);
  const [offer, setOffer] = useState<OAuthProvider[]>([]);
  const [oauthFlow, setOauthFlow] = useState<OAuthFlowState>({ status: 'idle' });

  // The offer is pulled once per open: the providers this build carries a
  // registration for, and nothing when it carries none. A refused pull
  // offers nothing rather than failing the wizard.
  useEffect(() => {
    if (!open) {
      return;
    }
    let live = true;
    oauthOffer().then(
      (answered) => {
        if (live) {
          setOffer(answered.providers);
        }
      },
      (err) => {
        console.error('account-setup: the oauth offer failed to pull', err);
      },
    );
    return () => {
      live = false;
      setOffer([]);
      setOauthFlow({ status: 'idle' });
    };
  }, [open]);

  // The completion subscription stands while the wizard is open, so it
  // is registered before any begin can return and no completion can
  // precede it. A result only lands on the flow it names: one the wizard
  // did not begin (a cancelled flow's late redirect) changes nothing.
  useEffect(() => {
    if (!open || !isTauri) {
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | null = null;
    onOauthSignInResult((result) => {
      setOauthFlow((flow) =>
        flow.status === 'browser-open' && flow.provider === result.provider
          ? { status: 'result', provider: result.provider, result }
          : flow,
      );
    }).then(
      (teardown) => {
        if (disposed) {
          // The wizard closed while the subscription was in flight; tear
          // the late registration down immediately.
          teardown();
        } else {
          unlisten = teardown;
        }
      },
      (err) => {
        console.error('account-setup: the sign-in result listener failed to register', err);
      },
    );
    return () => {
      disposed = true;
      unlisten?.();
      unlisten = null;
    };
  }, [open]);

  // The dialog owns its dismissal: Escape and a click on the backdrop's own
  // ground close it; the focus discipline rides the shared hook.
  const dialogRef = useDialogFocus<HTMLDivElement>(open);

  if (!open) {
    return null;
  }

  const close = () => {
    // Every detect still in flight belongs to a session that just ended;
    // its settlement must not touch whatever the next open shows.
    detectEpoch.current += 1;
    if (oauthFlow.status === 'browser-open') {
      void oauthCancel();
    }
    setStep('detect');
    setDetectBusy(false);
    setView(null);
    setNote(null);
    setMiss(null);
    setFields(EMPTY_FIELDS);
    setTested(null);
    setCertificate(null);
    setAccepted(null);
    setError(null);
    setOauthFlow({ status: 'idle' });
    useUiStore.getState().setAccountSetupOpen(false);
  };

  const goManual = (prefill: SetupFields, fallbackNote: string | null) => {
    setFields(prefill);
    setNote(fallbackNote);
    setMiss(null);
    setStep('manual');
  };

  const onDetect = async (email: string) => {
    const run = ++detectEpoch.current;
    setDetectBusy(true);
    try {
      const answered = await detectWithGuard(email);
      if (detectEpoch.current !== run) {
        return;
      }
      if ('Manual' in answered.recommendation) {
        // The miss reason is kept, not its sentence: the oauth-only miss
        // words itself by whether this build offers a provider sign-in,
        // and the offer can land after this click, so the note derives
        // at render from the reason.
        goManual(manualDefaults(email), null);
        setMiss(answered.recommendation.Manual.reason);
      } else {
        setView(answered);
        setStep('recommendation');
      }
    } catch (err) {
      if (detectEpoch.current !== run) {
        return;
      }
      goManual(manualDefaults(email), detectFallbackNote(err));
    } finally {
      if (detectEpoch.current === run) {
        setDetectBusy(false);
      }
    }
  };

  const onOAuthSignIn = (provider: OAuthProvider) => {
    setOauthFlow({ status: 'browser-open', provider });
    const hint = fields.username.trim();
    oauthBegin(provider, hint === '' ? undefined : hint).catch((err) => {
      // The browser never opened, so the flow never started: the state
      // comes home and the sentence shows beside the buttons.
      setOauthFlow((flow) =>
        flow.status === 'browser-open' && flow.provider === provider
          ? { status: 'idle', error: errorText(err) }
          : flow,
      );
    });
  };

  const onOAuthCancel = () => {
    void oauthCancel();
    setOauthFlow({ status: 'idle' });
  };

  const onManualFromDetect = (email: string) => goManual(manualDefaults(email), null);

  const onManualFromRecommendation = () => {
    if (view !== null) {
      goManual(manualPrefill(view), null);
    }
  };

  // The by-hand route carries only the address: nothing discovered crosses,
  // so an untrusted lookup's settings never reach the form through it.
  const onByHandFromRecommendation = () => {
    if (view !== null) {
      goManual(manualDefaults(recommendationEmail(view)), null);
    }
  };

  const set = (key: keyof SetupFields) => (value: string) => {
    setFields((prev) => ({ ...prev, [key]: value }));
    setTested(null);
  };

  async function runTest(setup: AccountSetup): Promise<void> {
    const outcome = await imapTestConnection(setup);
    if ('ok' in outcome) {
      setTested(outcome.ok.account_id);
      setCertificate(null);
      setError(null);
      return;
    }
    if ('rejected_certificate' in outcome) {
      setCertificate(outcome.rejected_certificate.certificate);
      setError(`the certificate did not verify: ${outcome.rejected_certificate.reason}`);
      return;
    }
    setCertificate(null);
    setError(outcome.failed.message);
  }

  const onTest = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await runTest(buildSetup(fields, accepted ?? undefined));
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const onAccept = async () => {
    if (certificate === null) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setAccepted(certificate);
      await runTest(buildSetup(fields, certificate));
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const onAdd = async () => {
    setBusy(true);
    setError(null);
    try {
      await addAccount(buildAccountToml(buildSetup(fields, accepted ?? undefined)));
      await dispatch(intents.refreshMail());
      close();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)]"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          close();
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="account-setup-title"
        ref={dialogRef}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            close();
          }
        }}
        className="w-[26rem] rounded-[var(--radius)] border border-border bg-card p-5 shadow-[var(--shadow-lg)] focus-visible:outline-none"
      >
        <h2 id="account-setup-title" className="mb-1 text-[15px] font-semibold text-card-foreground">
          Add an account
        </h2>
        <p className="mb-4 text-xs text-muted-foreground">
          The connection is tested before anything is stored.
        </p>

        {step === 'detect' && (
          <DetectStep
            busy={detectBusy}
            onCancel={close}
            onDetect={(email) => void onDetect(email)}
            onManual={onManualFromDetect}
          />
        )}

        {step === 'recommendation' && view !== null && (
          <RecommendationPanel
            view={view}
            onBack={() => setStep('detect')}
            onByHand={onByHandFromRecommendation}
            onManual={onManualFromRecommendation}
          />
        )}

        {step === 'manual' && (
          <>
            {(miss !== null || note !== null) && (
              <p role="status" className="mb-3 text-xs text-muted-foreground">
                {miss !== null ? missNote(miss, offer.length > 0) : note}
              </p>
            )}
            <div className="mb-3">
              <OAuthOffer
                offer={offer}
                flow={oauthFlow}
                onSignIn={onOAuthSignIn}
                onCancel={onOAuthCancel}
              />
            </div>
            <form onSubmit={onTest} className="grid gap-3">
              <label className="grid gap-1 text-xs text-muted-foreground">
                Mail server
                <input
                  value={fields.host}
                  onChange={(e) => set('host')(e.target.value)}
                  autoComplete="off"
                  spellCheck={false}
                  placeholder="imap.example.net"
                  className="rounded-full border border-input bg-background px-3 py-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
                />
              </label>
              <div className="grid grid-cols-2 gap-3">
                <label className="grid gap-1 text-xs text-muted-foreground">
                  Port
                  <input
                    value={fields.port}
                    onChange={(e) => set('port')(e.target.value)}
                    inputMode="numeric"
                    autoComplete="off"
                    className="rounded-full border border-input bg-background px-3 py-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
                  />
                </label>
                <label className="grid gap-1 text-xs text-muted-foreground">
                  Connection security
                  <select
                    value={fields.security}
                    onChange={(e) => set('security')(e.target.value)}
                    className="rounded-full border border-input bg-background px-3 py-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
                  >
                    <option value="implicit_tls">SSL/TLS</option>
                    <option value="start_tls">STARTTLS</option>
                  </select>
                </label>
              </div>
              <label className="grid gap-1 text-xs text-muted-foreground">
                Email address
                <input
                  value={fields.username}
                  onChange={(e) => set('username')(e.target.value)}
                  autoComplete="off"
                  spellCheck={false}
                  className="rounded-full border border-input bg-background px-3 py-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
                />
              </label>
              <label className="grid gap-1 text-xs text-muted-foreground">
                Password
                <input
                  type="password"
                  value={fields.password}
                  onChange={(e) => set('password')(e.target.value)}
                  autoComplete="new-password"
                  className="rounded-full border border-input bg-background px-3 py-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
                />
              </label>
              <p className="-mt-2 text-[11px] text-muted-foreground">
                Your password goes to the system keychain, never to a log file.
              </p>

              {certificate !== null && <CertificateOffer certificate={certificate} />}

              {error !== null && (
                <p role="alert" className="text-xs text-error">
                  {error}
                </p>
              )}
              {tested !== null && (
                <p role="status" className="text-xs text-success">The connection works. Ready to add.</p>
              )}

              <div className="mt-1 flex items-center justify-between gap-2">
                <button
                  type="button"
                  onClick={close}
                  className="rounded-[var(--radius-xs)] px-3 py-1.5 text-[13px] text-muted-foreground hover:bg-hover hover:text-foreground focus-visible:outline-none transition-colors focus-visible:border-primary"
                >
                  Cancel
                </button>
                <div className="flex items-center gap-2">
                  {certificate !== null && (
                    <button
                      type="button"
                      onClick={onAccept}
                      disabled={busy}
                      className="rounded-[var(--radius-xs)] bg-secondary px-3 py-1.5 text-[13px] text-secondary-foreground hover:bg-hover disabled:opacity-40 focus-visible:outline-none transition-colors focus-visible:border-primary"
                    >
                      Accept and continue
                    </button>
                  )}
                  <button
                    type="submit"
                    disabled={busy}
                    className="rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-40 focus-visible:outline-none transition-colors focus-visible:border-primary"
                  >
                    Test connection
                  </button>
                  <button
                    type="button"
                    onClick={onAdd}
                    disabled={busy || tested === null}
                    className="rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-40 focus-visible:outline-none transition-colors focus-visible:border-primary"
                  >
                    Add account
                  </button>
                </div>
              </div>
            </form>
          </>
        )}
      </div>
    </div>
  );
}
