// SPDX-License-Identifier: GPL-3.0-only
// The account setup flow: look up the server settings for one email address,
// then collect the mail server and its sign in, test the connection before
// anything is stored, offer a rejected certificate's fingerprint for a
// one-server exception, then add the account and start its first sync.
// Nothing is persisted until the test passes, and detection failing only ever
// lands on the manual form.

import { useRef, useState, type FormEvent } from 'react';

import {
  addAccount,
  dispatch,
  imapTestConnection,
  intents,
  type AccountSetup,
  type RecommendationView,
  type RejectedCertificateView,
} from '@/services/ipc';
import { useUiStore } from '@/stores/uiStore';
import { buildAccountToml } from './accountToml';
import { DetectStep } from './DetectStep';
import { RecommendationPanel } from './RecommendationPanel';
import {
  detectFallbackNote,
  detectWithGuard,
  EMPTY_FIELDS,
  manualDefaults,
  manualPrefill,
  missNote,
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

  if (!open) {
    return null;
  }

  const close = () => {
    // Every detect still in flight belongs to a session that just ended;
    // its settlement must not touch whatever the next open shows.
    detectEpoch.current += 1;
    setStep('detect');
    setDetectBusy(false);
    setView(null);
    setNote(null);
    setFields(EMPTY_FIELDS);
    setTested(null);
    setCertificate(null);
    setAccepted(null);
    setError(null);
    useUiStore.getState().setAccountSetupOpen(false);
  };

  const goManual = (prefill: SetupFields, fallbackNote: string | null) => {
    setFields(prefill);
    setNote(fallbackNote);
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
        goManual(manualDefaults(email), missNote(answered.recommendation.Manual.reason));
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

  const onManualFromDetect = (email: string) => goManual(manualDefaults(email), null);

  const onManualFromRecommendation = () => {
    if (view !== null) {
      goManual(manualPrefill(view), null);
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
    <div className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)]">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="account-setup-title"
        className="w-[26rem] rounded-[var(--radius)] border border-border bg-card p-5 shadow-[var(--shadow-lg)]"
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
          <RecommendationPanel view={view} onCancel={close} onManual={onManualFromRecommendation} />
        )}

        {step === 'manual' && (
          <>
            {note !== null && (
              <p role="status" className="mb-3 text-xs text-muted-foreground">
                {note}
              </p>
            )}
            <form onSubmit={onTest} className="grid gap-3">
              <label className="grid gap-1 text-xs text-muted-foreground">
                Mail server
                <input
                  value={fields.host}
                  onChange={(e) => set('host')(e.target.value)}
                  autoComplete="off"
                  spellCheck={false}
                  placeholder="imap.example.net"
                  className="rounded-[var(--radius-xs)] border border-input bg-background px-2 py-1.5 text-[13px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
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
                    className="rounded-[var(--radius-xs)] border border-input bg-background px-2 py-1.5 text-[13px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  />
                </label>
                <label className="grid gap-1 text-xs text-muted-foreground">
                  Connection security
                  <select
                    value={fields.security}
                    onChange={(e) => set('security')(e.target.value)}
                    className="rounded-[var(--radius-xs)] border border-input bg-background px-2 py-1.5 text-[13px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
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
                  className="rounded-[var(--radius-xs)] border border-input bg-background px-2 py-1.5 text-[13px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
                />
              </label>
              <label className="grid gap-1 text-xs text-muted-foreground">
                Password
                <input
                  type="password"
                  value={fields.password}
                  onChange={(e) => set('password')(e.target.value)}
                  autoComplete="new-password"
                  className="rounded-[var(--radius-xs)] border border-input bg-background px-2 py-1.5 text-[13px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
                />
              </label>

              {certificate !== null && (
                <div className="rounded-[var(--radius-xs)] border border-warning bg-accent p-3 text-xs">
                  <p className="font-medium text-foreground">
                    The server&apos;s certificate could not be verified
                  </p>
                  <dl className="mt-2 grid gap-1">
                    <div className="flex justify-between gap-2">
                      <dt className="text-muted-foreground">Server</dt>
                      <dd className="font-mono">{certificate.server_name}</dd>
                    </div>
                    <div className="flex justify-between gap-2">
                      <dt className="text-muted-foreground">Fingerprint</dt>
                      <dd className="break-all font-mono">{certificate.sha256}</dd>
                    </div>
                    {certificate.subject_common_name !== null && (
                      <div className="flex justify-between gap-2">
                        <dt className="text-muted-foreground">Claims to be</dt>
                        <dd className="font-mono">{certificate.subject_common_name}</dd>
                      </div>
                    )}
                  </dl>
                  <p className="mt-2 text-muted-foreground">
                    Accepting trusts this one certificate for this one server, and only here.
                  </p>
                </div>
              )}

              {error !== null && (
                <p role="alert" className="text-xs text-error">
                  {error}
                </p>
              )}
              {tested !== null && (
                <p className="text-xs text-success">The connection works. Ready to add.</p>
              )}

              <div className="mt-1 flex items-center justify-between gap-2">
                <button
                  type="button"
                  onClick={close}
                  className="rounded-[var(--radius-xs)] px-3 py-1.5 text-[13px] text-muted-foreground hover:bg-hover hover:text-foreground"
                >
                  Cancel
                </button>
                <div className="flex items-center gap-2">
                  {certificate !== null && (
                    <button
                      type="button"
                      onClick={onAccept}
                      disabled={busy}
                      className="rounded-[var(--radius-xs)] bg-secondary px-3 py-1.5 text-[13px] text-secondary-foreground hover:bg-hover disabled:opacity-40"
                    >
                      Accept and continue
                    </button>
                  )}
                  <button
                    type="submit"
                    disabled={busy}
                    className="rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-40"
                  >
                    Test connection
                  </button>
                  <button
                    type="button"
                    onClick={onAdd}
                    disabled={busy || tested === null}
                    className="rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-40"
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
