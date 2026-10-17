// SPDX-License-Identifier: GPL-3.0-only
// The oauth half of the IPC wire: the offer query (which providers this
// build carries a registration for), the begin and cancel calls, and the
// subscription the completion arrives on. Split from ipc.ts under the
// same re-export and line-cap split as the setup half before it, and
// re-exported whole through ipc, so ipc stays the one import point of
// the wire. The literals are pinned in oauth.test.ts against the JSON
// astra-desktop's commands_tests.rs asserts: the provider words cross as
// bare snake_case words, the successes as a bare `signed_in` or an
// externally tagged `added` record, the same tagged shape the intent wire
// uses.

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
 * The account a completed sign-in added: the address the host's lookup
 * named, and whether the grant withholds the calendar (the mail stands,
 * the agenda waits for a fresh sign-in).
 */
export interface OAuthAddedAccount {
  address: string;
  calendar_withheld: boolean;
}

/**
 * How one begun sign-in ended: the bare word `signed_in` for a provider
 * no account consumes yet, the added account for one the host finishes
 * into an account, or a refusal carrying the classification word beside
 * the host's own sentence as the diagnostic detail.
 */
export type OAuthSignInOutcome =
  | 'signed_in'
  | { added: OAuthAddedAccount }
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

/**
 * Whether the JMAP server for the address offers discoverable OAuth
 * sign-in: the advertise gate the JMAP form's button renders behind.
 * Never fails: a server that advertises nothing, or a probe that could
 * not ask, offers nothing.
 */
export function jmapOauthOffered(email: string, serverUrl: string): Promise<boolean> {
  return invoke('jmap_oauth_offered', { email, serverUrl });
}

/**
 * Begins one JMAP sign-in: the host discovers the server's
 * authorization server, registers this install, and opens the system
 * browser; this resolves once the browser opened, and the completion
 * arrives later, through {@link onJmapSignInResult}.
 */
export function jmapOauthBegin(email: string, serverUrl: string): Promise<void> {
  return invoke('jmap_oauth_begin', { email, serverUrl });
}

/**
 * How one begun JMAP sign-in ended: the added account (there is no bare
 * signed-in word; every completed flow finishes into an account), or a
 * refusal carrying the classification word beside the host's own
 * sentence as the diagnostic detail. Crosses with snake_case tags.
 */
export type JmapSignInOutcome =
  | { added: { address: string } }
  | { refused: { refusal: GrantRefusalWord; detail: string } };

/** The JMAP completion event's payload: how the flow ended. */
export interface JmapSignInResult {
  outcome: JmapSignInOutcome;
}

/**
 * Subscribes to the JMAP completion notification and resolves its
 * unlistener; the subscription discipline is the provider flows' own.
 */
export function onJmapSignInResult(
  handler: (result: JmapSignInResult) => void,
): Promise<() => void> {
  return listen<JmapSignInResult>('jmap_sign_in_result', (event) => {
    handler(event.payload);
  });
}
