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
import { foldersIntents } from './folders';
import type { FolderIntentBody } from './folders';
import type { CalendarIntentBody, InvitationBlock } from './calendar';
import { calendarIntents } from './calendar';
import type { PeopleIntentBody } from './people';
import { peopleIntents } from './people';
import type { SettingsIntentBody } from './settings';
import { settingsIntents } from './settings';
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

// The folder half (the folder intent family's carrying verbs and the
// move-to-folder batch), under the same re-export and split for the cap.
export * from './folders';

// The account-setup half (the connection and credential vocabulary, the
// setup form's fields, the detection recommendation views, and the three
// commands the setup flow dials), under the same re-export and split.
export * from './setup';

// The oauth half (the provider offer, the begin and cancel calls, and
// the subscription the sign-in completion arrives on), under the same
// re-export and split.
export * from './oauth';

// The calendar half (the snapshot mirror, the intent family's
// constructors, the snapshot pull, and the meeting brief call), under the
// same re-export and split.
export * from './calendar';

// The people half (the snapshot mirror, the intent family's constructors,
// the snapshot pull, the editor's card-seed read, and the recipient
// suggestions), under the same re-export and split.
export * from './people';

// The settings half (the snapshot mirror, the intent family's
// constructors, and the snapshot pull), under the same re-export and
// split.
export * from './settings';

// The mcp half (the advanced panel's status view and its three host
// commands), under the same re-export and split.
export * from './mcp';

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

// Types an intent and a snapshot share: the view mode and the search scope as
// the Rust protocol re-exports them; the row key stands in for its RowRef twin.
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
  | { archive_message: { row: RowRef } }
  | { move_to_trash: { row: RowRef } }
  | { move_messages: { rows: MessageRef[]; folder: FolderRef } }
  | { open_message: { message: MessageRef } }
  | 'close_reading'
  | { folders: FolderIntentBody }
  | { compose: ComposeIntent }
  | { submit_rich_mail: MailSubmission }
  | { submit_rich_reply: ReplySubmission }
  | { submit_rich_forward: ForwardSubmission }
  | { outbox: OutboxIntent }
  | { drafts: DraftsIntent }
  | { calendar: CalendarIntentBody }
  | { people: PeopleIntentBody }
  | { settings: SettingsIntentBody }
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
  /** Move one row (a message, or a whole conversation) into the archive. */
  archiveMessage: (row: RowRef): Intent => ({ archive_message: { row } }),
  /** Move one row (a message, or a whole conversation) into the trash. */
  moveToTrash: (row: RowRef): Intent => ({ move_to_trash: { row } }),
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
  // The folder family's carrying verbs and the move-to-folder batch, in the
  // enum's own order beside the create.
  ...foldersIntents,
  // The calendar family, appended after the search verbs in the enum's own
  // order.
  ...calendarIntents,
  // The people family, appended after the calendar family in the enum's
  // own order.
  ...peopleIntents,
  // The settings family, appended after the people family in the enum's
  // own order.
  ...settingsIntents,
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
  /**
   * What the open message means to this account's calendar, when it is a
   * scheduling message the calendar recognises: the card a surface mounts
   * above the body. Absent for every other message.
   */
  invitation?: InvitationBlock;
  /** The plugin decode's security banner, when a serving plugin had something to say. */
  security_banner?: import('./plugins').SecurityBannerView;
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
