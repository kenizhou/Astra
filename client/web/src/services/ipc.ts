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
