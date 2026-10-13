// SPDX-License-Identifier: GPL-3.0-only
// The oauth half of the IPC wire: the offer query (which providers this
// build carries a registration for), the begin and cancel calls, and the
// subscription the completion arrives on. Split from ipc.ts under the
// same re-export and line-cap split as the setup half before it, and
// re-exported whole through ipc, so ipc stays the one import point of
// the wire. The literals are pinned in oauth.test.ts against the JSON
// astra-desktop's commands_tests.rs asserts: the provider words cross as
// bare snake_case words and the success outcome as a bare `signed_in`,
// the same externally tagged shape the intent wire uses.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** A provider whose sign-in this build can offer; crosses as a bare word. */
export type OAuthProvider = 'microsoft' | 'google';

/** The offer: which providers this build can sign in with. */
export interface OAuthOfferView {
  providers: OAuthProvider[];
}

/** What `oauthBegin` answers once the browser opened. */
export interface BrowserOpened {
  provider: OAuthProvider;
}

/** Why a sign-in refused, as the ported classification reads it. */
export type GrantRefusalWord = 'dead' | 'underscoped' | 'indeterminate';

/**
 * How one begun sign-in ended: the bare word `signed_in`, or a refusal
 * carrying the classification word beside the host's own sentence as
 * the diagnostic detail.
 */
export type OAuthSignInOutcome =
  | 'signed_in'
  | { refused: { refusal: GrantRefusalWord; detail: string } };

/** The completion event's payload: the provider and how its sign-in ended. */
export interface OAuthSignInResult {
  provider: OAuthProvider;
  outcome: OAuthSignInOutcome;
}

/**
 * Pulls the offer: the providers this build carries a registration for,
 * empty when it carries none, so the wizard's provider routes never
 * render.
 */
export function oauthOffer(): Promise<OAuthOfferView> {
  return invoke('oauth_offer');
}

/**
 * Begins one provider sign-in: the host mints the request, opens the
 * system browser, and this resolves once the browser opened. The
 * completion arrives later, through {@link onOauthSignInResult}; the
 * `loginHint` (the address being connected, when known) pre-fills and
 * targets that account at the provider.
 */
export function oauthBegin(provider: OAuthProvider, loginHint?: string): Promise<BrowserOpened> {
  return invoke('oauth_begin', { provider, loginHint });
}

/** Cancels the standing sign-in, whichever provider began it. */
export function oauthCancel(): Promise<void> {
  return invoke('oauth_cancel');
}

/**
 * Subscribes to the completion notification and resolves its
 * unlistener. The wizard subscribes while it is open, before any begin
 * can return, so no completion can precede the subscription.
 */
export function onOauthSignInResult(
  handler: (result: OAuthSignInResult) => void,
): Promise<() => void> {
  return listen<OAuthSignInResult>('oauth_sign_in_result', (event) => {
    handler(event.payload);
  });
}
