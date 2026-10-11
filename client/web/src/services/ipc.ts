// SPDX-License-Identifier: GPL-3.0-only
// The typed IPC layer over the desktop host's command surface. The wire shapes
// mirror astra-app's serde exactly: intents are externally tagged with
// snake_case tags, unit variants cross as bare renamed words, and the snapshot
// types come from surfaces.ts. ipc.test.ts pins every intent against the
// literal JSON astra-app's protocol_tests.rs asserts, so neither side drifts.

import { invoke } from '@tauri-apps/api/core';

import type { ComposerDocument } from './composerDocument';
import { composeIntents } from './composeRequest';
import type {
  ComposeIntent,
  ComposeRequestSnapshot,
  ForwardSubmission,
  ReplySubmission,
} from './composeRequest';
import { sendingIntents } from './sending';
import type { MailSubmission, OutboxIntent, SendingSnapshot } from './sending';
import type { MailboxListSnapshot, RowKey, SearchScope, SyncProgressSnapshot, ViewMode } from './surfaces';

// The composer document's whole type set, re-exported so this module stays the
// one import point of the wire.
export * from './composerDocument';

// The compose half (the compose family, its two threaded submits, and the
// request surface's snapshot types), under the same re-export and split out
// for the line cap.
export * from './composeRequest';

// The sending half (the submit payload, the outbox family, and the queue's
// snapshot types), under the same re-export and split out for the line cap.
export * from './sending';

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

// Types an intent and a snapshot share, re-exported here exactly as the Rust
// protocol re-exports them: the row key, the view mode, the search scope.
export type { RowKey, SearchScope, ViewMode };

/** One mailbox-list row an action names: a message, or a whole conversation. */
export type RowRef = RowKey;

/** One open composition, named by the client when it opens the composer. */
export type CompositionId = string;

/** The fields a draft save carries, beside the composer document itself. */
export interface DraftSave {
  /** Which composer this is, so the save supersedes its own previous copy. */
  composition: CompositionId;
  /** The account to store it under: the composer's From dropdown. */
  from: string;
  /** The `To` recipients, comma-separated; may be empty. */
  to: string;
  /** The `Cc` recipients, comma-separated; may be empty. */
  cc: string;
  /** The `Bcc` recipients, comma-separated; may be empty. */
  bcc: string;
  /** The subject line; may be empty. */
  subject: string;
  /** The shared composer document. */
  document: ComposerDocument;
  /** Forget the composition once the save settles: the composer is leaving. */
  then_close: boolean;
}

/**
 * What the composer asks the runtime to do with the draft it is holding,
 * nested under the `drafts` tag of the intent that carries it.
 */
export type DraftsIntent =
  | { save: DraftSave }
  | { discard: { composition: CompositionId } }
  | { close: { composition: CompositionId } };

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
  | { open_message: { message: MessageRef } }
  | 'close_reading'
  | { folders: { create: { account: string; name: string; parent?: string } } }
  | { compose: ComposeIntent }
  | { submit_rich_mail: MailSubmission }
  | { submit_rich_reply: ReplySubmission }
  | { submit_rich_forward: ForwardSubmission }
  | { outbox: OutboxIntent }
  | { drafts: DraftsIntent }
  | { search: { query: string | null } }
  | { set_search_scope: { scope: SearchScope } };

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
  /** Read one message in the reading pane: fetch its body and publish it. */
  openMessage: (message: MessageRef): Intent => ({ open_message: { message } }),
  /** Shut the reading pane; navigating the list leaves it as it was. */
  closeReading: (): Intent => 'close_reading',
  /** Make a folder, at the top of the account or inside a parent. */
  createFolder: (account: string, name: string, parent?: string): Intent => ({
    folders: {
      create: parent === undefined ? { account, name } : { account, parent, name },
    },
  }),
  // The compose family, in the enum's own order between the folders and the
  // submits, with the slice's two threaded submits beside it.
  ...composeIntents,
  // The submit and the outbox family, in the enum's own order between the
  // compose family and the drafts.
  ...sendingIntents,
  /** Store the composer's current content in the account's Drafts folder. */
  saveDraft: (draft: DraftSave): Intent => ({
    drafts: {
      save: {
        composition: draft.composition,
        from: draft.from,
        to: draft.to,
        cc: draft.cc,
        bcc: draft.bcc,
        subject: draft.subject,
        document: draft.document,
        then_close: draft.then_close,
      },
    },
  }),
  /** Remove the composition's stored draft and forget the composition. */
  discardDraft: (composition: CompositionId): Intent => ({
    drafts: { discard: { composition } },
  }),
  /** Save what the composition last handed over, then forget it. */
  closeDraft: (composition: CompositionId): Intent => ({
    drafts: { close: { composition } },
  }),
  /** Search the mail for the query, or clear the search with null. */
  search: (query: string | null): Intent => ({ search: { query } }),
  /** Narrow the search to a scope and re-run it over the live query. */
  setSearchScope: (scope: SearchScope): Intent => ({ set_search_scope: { scope } }),
};

/**
 * The identity of the one message a reading snapshot holds the body of: the
 * account and provider key pair, the same shape a message row of
 * `RowKey` carries standing alone.
 */
export type MessageKey = Pick<MessageRef, 'account' | 'key'>;

/** One downloadable attachment shown beneath the open message's body. */
export interface AttachmentRow {
  /** The attachment's message-scoped id: what a save or a download names. */
  id: number;
  /** The suggested file name, from the part's headers. */
  file_name: string;
  /** The part's media type, e.g. `application/pdf`. */
  media_type: string;
  /** The decoded byte length. */
  size: number;
}

/**
 * The reading pane's snapshot: the one open message's sanitised body. `html`
 * is already sanitised and every remote reference in it is a placeholder, so
 * nothing remote loads unless the reader opts in. Both `html` and `plain` are
 * absent when the message carries no text body at all, and `attachments` when
 * it carries none.
 */
export interface ReadingSnapshot {
  /** The identity of the message this body belongs to, to match the open row. */
  message: MessageKey;
  /** The subject, empty if none. */
  subject: string;
  /** The sender's display name, empty when the header carried none. */
  from_name: string;
  /** The sender's bare email address, empty when there is no sender. */
  from_address: string;
  /** When the message was sent, as a whole-second UTC `…Z` instant string. */
  sent_at: string;
  /** The sanitised HTML body, when the message carries an HTML part. */
  html?: string;
  /** The plain-text body, when the message carries one. */
  plain?: string;
  /** The downloadable attachments, absent when there are none. */
  attachments?: AttachmentRow[];
  /** How many remote references the sanitised HTML carries, all blocked. */
  remote_image_count: number;
}

/** The state of a composition's most recent draft save. */
export type DraftSaveState = 'saving' | 'saved';

/**
 * One composition's most recent draft save, as the composer renders its quiet
 * hint. `composition` names whose save moved, so a surface matching it against
 * its own composer discards a stale one.
 */
export interface DraftStatusSnapshot {
  /** The composition whose save moved, exactly as the client minted its id. */
  composition: string;
  /** How the most recent save of that composition ended up. */
  state: DraftSaveState;
  /** When the save landed, as a whole-second UTC instant string; empty in flight. */
  saved_at: string;
}

/**
 * A stored draft, as the composer that resumes it needs to open: the fields to
 * hydrate and the body as plain text.
 */
export interface ResumeView {
  /** The account whose Drafts folder the draft was in. */
  account: string;
  /** The `To` field, comma-joined. */
  to: string;
  /** The `Cc` field, comma-joined. */
  cc: string;
  /** The `Bcc` field, comma-joined; only what the stored copy carried. */
  bcc: string;
  /** The subject. */
  subject: string;
  /** The body as plain text: what a composer opens without a markup parser. */
  body_text: string;
}

/** One smart-reply chip: the whole drafted reply beside its one-line preview. */
export interface ChipView {
  /** The drafted reply, trimmed: the text the accept path prefills. */
  reply: string;
  /** The reply's head as one line, capped at eighty characters with `...` when
   * it was trimmed. */
  preview: string;
}

/**
 * The draft-pool event's payload: which message gained a pool row, named by
 * the same account and key pair the reading snapshot's identity carries, so
 * the pane matches it against its open row before pulling chips.
 */
export interface DraftPoolLanded {
  /** The account owning the answered message. */
  account: string;
  /** The answered message's provider key, unique only within `account`. */
  key: string;
}

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

/** Pulls the reading snapshot: the open message's body, or null when closed. */
export function reading(): Promise<ReadingSnapshot | null> {
  return invoke('reading');
}

/** Pulls the draft-status snapshot: the composer's save hint, or null when none. */
export function draftStatus(): Promise<DraftStatusSnapshot | null> {
  return invoke('draft_status');
}

/**
 * Opens the account's latest stored draft into the composition the caller
 * minted for the composer it is about to open; null when there is no draft.
 */
export function draftResume(
  account: string,
  composition: CompositionId,
): Promise<ResumeView | null> {
  return invoke('draft_resume', { account, composition });
}

/** Pulls the sending snapshot: the outbox panel's rows, empty when every send has gone. */
export function sending(): Promise<SendingSnapshot> {
  return invoke('sending');
}

/**
 * Pulls the compose-request snapshot: the prefill a reply or forward opened
 * the composer on, or null when no request is pending.
 */
export function composeRequest(): Promise<ComposeRequestSnapshot | null> {
  return invoke('compose_request');
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
