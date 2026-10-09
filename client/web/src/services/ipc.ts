// SPDX-License-Identifier: GPL-3.0-only
// The typed IPC layer over the desktop host's command surface. The wire shapes
// mirror astra-app's serde exactly: intents are externally tagged with
// snake_case tags, unit variants cross as bare renamed words, and the snapshot
// types come from surfaces.ts. ipc.test.ts pins every intent against the
// literal JSON astra-app's protocol_tests.rs asserts, so neither side drifts.

import { invoke } from '@tauri-apps/api/core';

import type { MailboxListSnapshot, SyncProgressSnapshot } from './surfaces';

/** A folder bound to its owning account; unique only within the account. */
export interface FolderRef {
  account: string;
  key: string;
}

/** A message bound to its owning account by its provider key. */
export interface MessageRef {
  account: string;
  key: string;
}

/** A conversation bound to its owning account by its thread id. */
export interface ThreadRef {
  account: string;
  thread_id: string;
}

/** One mailbox-list row an action names: a message, or a whole conversation. */
export type RowRef = { message: MessageRef } | { thread: ThreadRef };

/** How the mailbox list is grouped, crossing as a plain word. */
export type ViewMode = 'flat' | 'threaded';

/**
 * One host intent: the single inbound channel of the unidirectional loop. A
 * bare string is a unit variant; a single keyed object is a carrying variant.
 * Built through the `intents` constructors so the key order matches the Rust
 * wire byte for byte.
 */
export type Intent =
  | 'refresh_mail'
  | 'show_more'
  | { select_account: string | null }
  | { set_account_expanded: { account: string; expanded: boolean } }
  | { set_folder_expanded: { folder: FolderRef; expanded: boolean } }
  | { select_folder: { folder: FolderRef } }
  | { set_view_mode: ViewMode }
  | { mark_read: { row: RowRef; read: boolean } }
  | { set_flagged: { row: RowRef; flagged: boolean } }
  | { folders: { create: { account: string; name: string; parent?: string } } };

/** Smart constructors for every intent, in the enum's own order. */
export const intents = {
  /** Sync the account's mail and refresh the mailbox-list snapshot. */
  refreshMail: (): Intent => 'refresh_mail',
  /** Show one account's folders, or the unified view for null. */
  selectAccount: (account: string | null): Intent => ({ select_account: account }),
  /** Open or shut one account's folder tree in the sidebar. */
  setAccountExpanded: (account: string, expanded: boolean): Intent => ({
    set_account_expanded: { account, expanded },
  }),
  /** Open or shut the folders filed inside one folder. */
  setFolderExpanded: (folder: FolderRef, expanded: boolean): Intent => ({
    set_folder_expanded: { folder, expanded },
  }),
  /** Show one folder's mail. */
  selectFolder: (folder: FolderRef): Intent => ({ select_folder: { folder } }),
  /** Switch the mailbox list between a flat and a threaded view. */
  setViewMode: (mode: ViewMode): Intent => ({ set_view_mode: mode }),
  /** Grow the visible mailbox-list window by one page. */
  showMore: (): Intent => 'show_more',
  /** Mark one row (a message, or a whole conversation) read or unread. */
  markRead: (row: RowRef, read: boolean): Intent => ({ mark_read: { row, read } }),
  /** Flag or unflag one row (a message, or a whole conversation). */
  setFlagged: (row: RowRef, flagged: boolean): Intent => ({
    set_flagged: { row, flagged },
  }),
  /** Make a folder, at the top of the account or inside a parent. */
  createFolder: (account: string, name: string, parent?: string): Intent => ({
    folders: {
      create: parent === undefined ? { account, name } : { account, parent, name },
    },
  }),
};

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

/** Dispatches one intent onto the app's runtime. */
export function dispatch(intent: Intent): Promise<void> {
  return invoke('dispatch', { intent });
}

/** Pulls the mailbox-list snapshot. */
export function mailboxList(): Promise<MailboxListSnapshot> {
  return invoke('mailbox_list');
}

/** Pulls the sync-progress snapshot. */
export function syncProgress(): Promise<SyncProgressSnapshot> {
  return invoke('sync_progress');
}

/** Adds one account from its config TOML: dials, persists, first sync. */
export function addAccount(configToml: string): Promise<AddedAccount> {
  return invoke('add_account', { configToml });
}

/** Tests one setup's connection once, before anything is stored. */
export function imapTestConnection(setup: AccountSetup): Promise<ImapTestOutcome> {
  return invoke('imap_test_connection', { setup });
}

/** Detects the mail-server settings for one email, bounded to five seconds. */
export function detectAccountSettings(email: string): Promise<RecommendationView> {
  return invoke('detect_account_settings', { email });
}
