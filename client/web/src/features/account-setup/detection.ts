// SPDX-License-Identifier: GPL-3.0-only
// The detection step's model: the client-side bound around the host's detect
// command, and the mappings from a recommendation to the manual form the
// wizard falls back on. The host already bounds detection to five seconds;
// the guard here is the belt-and-braces pair that never leaves the wizard
// waiting on a lost answer.

import {
  detectAccountSettings,
  type ConnectionSecurity,
  type MissReason,
  type RecommendationView,
  type SetupRecommendation,
} from '@/services/ipc';

// The miss reason, re-exported so the flow keeps its one import point;
// the note derives from it at render, not at the click that recorded it.
export type { MissReason };

/** The fields the manual form collects, shared with the flow that renders it. */
export interface SetupFields {
  host: string;
  port: string;
  username: string;
  password: string;
  security: ConnectionSecurity;
}

export const EMPTY_FIELDS: SetupFields = {
  host: '',
  port: '993',
  username: '',
  password: '',
  security: 'implicit_tls',
};

/** The client bound, mirroring the host's own five second detect deadline. */
export const DETECT_BOUND_MS = 5000;

export const TIMEOUT_NOTE = 'Detection did not answer within five seconds; enter the server by hand.';

/** The client guard's own rejection, so a timeout never reads as a failure. */
class DetectTimeout extends Error {}

/** The manual form's fields when nothing detected can prefill them. */
export function manualDefaults(email: string): SetupFields {
  return { ...EMPTY_FIELDS, username: email };
}

/**
 * Runs one detection under the client bound: the first of the host's answer
 * or the timeout wins, and the timer never outlives the race.
 */
export function detectWithGuard(email: string): Promise<RecommendationView> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const guard = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new DetectTimeout(TIMEOUT_NOTE)), DETECT_BOUND_MS);
  });
  return Promise.race([detectAccountSettings(email), guard]).finally(() => clearTimeout(timer));
}

/** The note the manual form carries when detection could not answer. */
export function detectFallbackNote(err: unknown): string {
  if (err instanceof DetectTimeout) {
    return TIMEOUT_NOTE;
  }
  const message = err instanceof Error ? err.message : String(err);
  return `Detection failed: ${message}; enter the server by hand.`;
}

/** The note for a detection that answered with a manual miss. */
export function missNote(reason: MissReason, providerSignIn: boolean): string {
  switch (reason) {
    case 'NetworkError':
      return 'Detection could not reach the network; enter the server by hand.';
    case 'OauthOnlyProvider':
      return providerSignIn
        ? 'This provider only offers browser sign-in; sign in with the provider, or enter the server by hand.'
        : 'This provider only offers browser sign-in, which this build does not offer yet; enter the server by hand.';
    default:
      return 'No settings were found for this address; enter the server by hand.';
  }
}

/** The address every carrying variant holds; the manual miss has none. */
function variantEmail(recommendation: SetupRecommendation): string {
  const variant = Object.values(recommendation)[0] as { email?: string };
  return variant?.email ?? '';
}

/**
 * The manual form's fields behind a recommendation's manual toggle: a found
 * IMAP server prefills the host, port, security, and username it detected;
 * every other route leaves the manual defaults, carrying its address over.
 */
export function manualPrefill(view: RecommendationView): SetupFields {
  const rec = view.recommendation;
  if (!('Imap' in rec)) {
    return manualDefaults(variantEmail(rec));
  }
  const imap = rec.Imap;
  return {
    host: imap.incoming.hostname,
    port: String(imap.incoming.port),
    username: imap.incoming.username === '' ? imap.email : imap.incoming.username,
    password: '',
    security: imap.imap_security,
  };
}
