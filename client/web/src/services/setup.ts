// SPDX-License-Identifier: GPL-3.0-only
// The account-setup half of the IPC wire: the connection and credential
// vocabulary, the setup form's fields, the detection recommendation views,
// and the three host commands the setup flow dials (the only commands that
// dial). Split from ipc.ts to keep that file under the line cap, like the
// compose and sending halves before it, and re-exported whole through it, so
// ipc stays the one import point of the wire. The literals are pinned in
// ipc.test.ts against the JSON astra-desktop's detection_tests.rs and
// commands_tests.rs assert.

import { invoke } from '@tauri-apps/api/core';

/** How a mail connection is secured, crossing as a snake_case word. */
export type ConnectionSecurity = 'implicit_tls' | 'start_tls';

/** What the account authenticates with: a typed password or a sign-in grant. */
export type SetupCredential = { password: string } | 'grant';

/** A server certificate accepted after being shown, scoped to one server. */
export interface CertificateException {
  server_name: string;
  sha256: string;
}

/** The fields the setup form collects, as the host's test command reads them. */
export interface AccountSetup {
  imap_host: string;
  username: string;
  credential: SetupCredential;
  smtp_host?: string;
  caldav_base_url?: string;
  imap_security: ConnectionSecurity;
  smtp_security: ConnectionSecurity;
  accepted_certificate?: CertificateException;
}

/** The record an added account returns. */
export interface AddedAccount {
  account_id: string;
  email: string;
}

/** A server certificate that did not verify, as the setup flow shows it. */
export interface RejectedCertificateView {
  server_name: string;
  sha256: string;
  subject_common_name: string | null;
  subject_organisation: string | null;
  issuer_common_name: string | null;
  issuer_organisation: string | null;
  not_before: number | null;
  not_after: number | null;
}

/** The verdict of one setup connection test, as the setup flow reads it. */
export type ImapTestOutcome =
  | { ok: { account_id: string } }
  | { rejected_certificate: { certificate: RejectedCertificateView; reason: string } }
  | { failed: { message: string } };

/** Why detection routed to manual setup; crosses as a bare word. */
export type MissReason = 'InvalidEmail' | 'NothingFound' | 'NetworkError' | 'OauthOnlyProvider';

/** A detected server rendered for the confirmation card. */
export interface ServerSummary {
  protocol: string;
  hostname: string;
  port: number;
  security: string;
  username: string;
}

/** The JMAP route's prefilled form. */
export interface JmapRecommendation {
  email: string;
  server_url: string;
  is_trusted: boolean;
  source: string;
  caldav_url?: string;
  carddav_url?: string;
}

/** The password form's prefilled IMAP/SMTP settings. */
export interface ImapRecommendation {
  email: string;
  imap_host: string;
  smtp_host?: string;
  imap_security: ConnectionSecurity;
  smtp_security: ConnectionSecurity;
  incoming: ServerSummary;
  outgoing?: ServerSummary;
  caldav_url?: string;
  carddav_url?: string;
  oauth_issuer?: string;
  is_trusted: boolean;
  source: string;
}

/** The calendar-and-contacts-only route. */
export interface DavRecommendation {
  email: string;
  caldav_url?: string;
  carddav_url?: string;
}

/**
 * Where the email-first setup step routes, with everything the target form
 * needs prefilled. The variants cross externally tagged, names as declared on
 * the Rust side; optional fields are absent rather than null when unset.
 */
export type SetupRecommendation =
  | { Jmap: JmapRecommendation }
  | { Imap: ImapRecommendation }
  | { Microsoft: { email: string } }
  | { Google: { email: string } }
  | { Dav: DavRecommendation }
  | { Manual: { reason: MissReason } };

/** One use an account can be put to; crosses as a bare word. */
export type Capability = 'Mail' | 'Calendar' | 'Contacts' | 'Colleagues';

/** One use a setup route offers, as the wizard renders its toggles. */
export interface SetupChoice {
  capability: Capability;
  on: boolean;
  server_found: boolean;
}

/** The lookup strategy a detection result came from; crosses as a bare word. */
export type SourceKind =
  | 'JmapWellKnown'
  | 'JmapSrv'
  | 'Autoconfig'
  | 'AutoconfigWellKnown'
  | 'Ispdb'
  | 'ImapSrv'
  | 'MxAutoconfig'
  | 'MxIspdb';

/** What one detection run found, before routing. */
export type DetectedOutcome = 'jmap' | 'mail' | 'dav' | { nothing: { network_error: boolean } };

/** The summary line of a detection run: the outcome and the winning strategy. */
export interface DetectedSummary {
  outcome: DetectedOutcome;
  source_kind: SourceKind | null;
}

/** The result of one detection run, as the setup flow reads it. */
export interface RecommendationView {
  recommendation: SetupRecommendation;
  choices: SetupChoice[];
  detected: DetectedSummary;
}

/** Adds one account from its config TOML: dials, persists, first sync. */
export function addAccount(configToml: string): Promise<AddedAccount> {
  return invoke('add_account', { configToml });
}

/**
 * Adds one JMAP account from the form's fields: the host derives the
 * base URL by the one rule every stored document walks (a blank server
 * derives the email's domain, a bare host gains `https://`), builds the
 * document, and runs the same dial-before-persist transaction. The one
 * secret crosses as `password` because that form is the strictly more
 * capable one.
 */
export function addJmapAccount(
  email: string,
  serverUrl: string,
  password: string,
): Promise<AddedAccount> {
  return invoke('add_jmap_account', { email, serverUrl, password });
}

/** Tests one setup's connection once, before anything is stored. */
export function imapTestConnection(setup: AccountSetup): Promise<ImapTestOutcome> {
  return invoke('imap_test_connection', { setup });
}

/** Detects the mail-server settings for one email, bounded to five seconds. */
export function detectAccountSettings(email: string): Promise<RecommendationView> {
  return invoke('detect_account_settings', { email });
}
