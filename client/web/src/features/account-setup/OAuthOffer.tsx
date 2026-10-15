// SPDX-License-Identifier: GPL-3.0-only
// The OAuth offer beside the wizard's password form: one sign-in button
// per provider this build carries a registration for, the browser-open
// state with its cancel while a flow stands, and the completion
// sentences. The offer renders nothing when the build offers no
// provider, which is every build until a registration is injected. The
// refusal sentences hang off the ported classification words and never
// off the host's detail text, which rides beside them as the diagnostic.

import type {
  GrantRefusalWord,
  OAuthProvider,
  OAuthSignInResult,
  OAuthSignInOutcome,
} from '@/services/ipc';

/** The wizard's OAuth state machine: idle, browser-open, then the result. */
export type OAuthFlowState =
  | { status: 'idle'; error?: string }
  | { status: 'browser-open'; provider: OAuthProvider }
  | { status: 'result'; provider: OAuthProvider; result: OAuthSignInResult };

/** The provider's display name, as the buttons and sentences use it. */
export function providerLabel(provider: OAuthProvider): string {
  return provider === 'microsoft' ? 'Microsoft' : 'Google';
}

/**
 * The success sentence: an account the host finished adding names its
 * address and says where its first sync runs; the bare signed-in word
 * stays the honest sentence for a provider no account consumes yet, whose
 * account type arrives with its slice.
 */
export function successNote(provider: OAuthProvider, outcome: OAuthSignInOutcome): string {
  if (typeof outcome !== 'object' || !('added' in outcome)) {
    const name = providerLabel(provider);
    return `Signed in with ${name}. This build cannot add a ${name} account yet; the account type arrives with the next update.`;
  }
  const base = `Signed in with ${providerLabel(provider)} as ${outcome.added.address}. The account joins with its first sync in the background.`;
  return outcome.added.calendar_withheld
    ? `${base} The calendar is missing from this sign-in; sign in again and consent to the calendar permission.`
    : base;
}

/** The refusal sentence for one classification word. */
export function refusalNote(refusal: GrantRefusalWord, provider: OAuthProvider): string {
  switch (refusal) {
    case 'dead':
      return `${providerLabel(provider)} reports this sign-in expired or withdrawn. Sign in again; the consent screen will reappear.`;
    case 'underscoped':
      return 'The consent given misses a permission this build needs. Sign in again and consent to the full list.';
    case 'indeterminate':
      return 'The sign-in could not be completed; reaching the provider failed. Nothing changed about any sign-in that stands.';
  }
}

interface OAuthOfferProps {
  offer: OAuthProvider[];
  flow: OAuthFlowState;
  onSignIn: (provider: OAuthProvider) => void;
  onCancel: () => void;
}

const BUTTON_CLASS =
  'rounded-[var(--radius-xs)] bg-secondary px-3 py-1.5 text-[13px] text-secondary-foreground hover:bg-hover';

export function OAuthOffer({ offer, flow, onSignIn, onCancel }: OAuthOfferProps) {
  if (offer.length === 0) {
    return null;
  }

  if (flow.status === 'browser-open') {
    return (
      <div className="grid gap-2 rounded-[var(--radius-xs)] border border-border p-3">
        <p role="status" className="text-xs text-muted-foreground">
          The browser is open; finish signing in with {providerLabel(flow.provider)} there.
        </p>
        <div>
          <button type="button" onClick={onCancel} className={BUTTON_CLASS}>
            Cancel sign-in
          </button>
        </div>
      </div>
    );
  }

  // A completed sign-in has nothing more to offer: the added account
  // stands and its first sync runs, and the bare-word success belongs to
  // a provider no account consumes yet. A refusal brings the buttons
  // back, because both consent-asking refusals are remedied by beginning
  // again.
  const refused =
    flow.status === 'result' &&
    typeof flow.result.outcome === 'object' &&
    !('added' in flow.result.outcome)
      ? flow.result.outcome.refused
      : null;
  if (flow.status === 'result' && refused === null) {
    return (
      <div className="grid gap-2 rounded-[var(--radius-xs)] border border-border p-3">
        <p role="status" className="text-xs text-success">
          {successNote(flow.provider, flow.result.outcome)}
        </p>
      </div>
    );
  }

  const detail = refused !== null ? refused.detail : null;

  return (
    <div className="grid gap-2 rounded-[var(--radius-xs)] border border-border p-3">
      <p className="text-xs text-muted-foreground">Or sign in with your provider</p>
      <div className="flex flex-wrap items-center gap-2">
        {offer.map((provider) => (
          <button
            key={provider}
            type="button"
            onClick={() => onSignIn(provider)}
            className={BUTTON_CLASS}
          >
            Sign in with {providerLabel(provider)}
          </button>
        ))}
      </div>
      {flow.status === 'idle' && flow.error !== undefined && (
        <p role="alert" className="text-xs text-error">
          {flow.error}
        </p>
      )}
      {refused !== null && flow.status === 'result' && (
        <>
          <p role="status" className="text-xs text-error">
            {refusalNote(refused.refusal, flow.result.provider)}
          </p>
          {detail !== null && <p className="text-xs text-muted-foreground">{detail}</p>}
        </>
      )}
    </div>
  );
}
