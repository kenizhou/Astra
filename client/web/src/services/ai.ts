// SPDX-License-Identifier: GPL-3.0-only
// The AI replies and assist IPC layer: the command wrappers the reading
// pane's chips, summary, style, and translate surfaces call, the accept
// that opens the composer on a drafted reply, the draft-pool event
// subscription that tells the pane a message's pool changed, and the A3
// assist pulls (the knowledge listing, extraction, and forget, the
// snippet listing and search, the compose autocomplete). The wire shapes
// mirror the desktop host's serde exactly: a chip crosses as its reply
// beside its preview, a knowledge entry as its six fields, the
// extraction as its three counts, a snippet as its five, and the event
// payload names the landed message by its account and key pair.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { ChipView, DraftPoolLanded, MessageRef } from './ipc';

// The wire types this service carries, re-exported so this module stays the
// one import point of the AI replies surface.
export type { ChipView, DraftPoolLanded };

/** One knowledge entry as the knowledge surface lists it, newest first. */
export interface KnowledgeView {
  /** The entry's identity: what a forget names. */
  id: string;
  /** The entry's kind word: fact, preference, project, contact, persona. */
  kind: string;
  /** The entry's text, one short third-person sentence about the user. */
  text: string;
  /** The account owning the message the entry was extracted from. */
  source_account: string;
  /** The source message's provider key, unique within its account. */
  source_key: string;
  /** The store's creation stamp (unix seconds). */
  created_at: number;
}

/** One extraction run's summary: what landed, what skipped, what rode. */
export interface ExtractionSummary {
  /** How many entries landed in the store. */
  extracted: number;
  /** How many entries the exact-text dedup guard skipped. */
  skipped_duplicates: number;
  /** How many messages the batch carried. */
  messages_considered: number;
}

/** One snippet as the picker lists and searches it. */
export interface SnippetView {
  /** The snippet's identity. */
  id: string;
  /** The snippet's short title, the picker's label. */
  title: string;
  /** The snippet's body, the text an insert places at the cursor. */
  body: string;
  /** The store's creation stamp (unix seconds). */
  created_at: number;
  /** The store's touch stamp (unix seconds); the listing's order rides it. */
  updated_at: number;
}

/** The translate action's target language, the fixed six as snake_case words. */
export type TranslateLanguage =
  | 'english'
  | 'chinese'
  | 'german'
  | 'french'
  | 'japanese'
  | 'spanish';

/** Lists one account's knowledge entries, newest first. */
export function knowledgeList(account: string): Promise<KnowledgeView[]> {
  return invoke('knowledge_list', { account });
}

/**
 * Runs one knowledge extraction over the account's most recent mail and
 * answers the run's summary once the run is done; `limit` caps the batch
 * below the host's thirty when named.
 */
export function knowledgeExtract(account: string, limit?: number): Promise<ExtractionSummary> {
  return limit === undefined
    ? invoke('knowledge_extract', { account })
    : invoke('knowledge_extract', { account, limit });
}

/** Forgets one knowledge entry, answering whether any row went. */
export function knowledgeForget(id: string): Promise<boolean> {
  return invoke('knowledge_forget', { id });
}

/** Lists the snippet library, the most recently touched first. */
export function snippetList(): Promise<SnippetView[]> {
  return invoke('snippet_list');
}

/** Searches the snippet library: the ranked answer with its substring fallback. */
export function snippetSearch(query: string): Promise<SnippetView[]> {
  return invoke('snippet_search', { query });
}

/**
 * Asks the compose autocomplete for the continuation at the cursor; the
 * trailing half is optional and omitted when the cursor sits at the end.
 * An empty answer never shows a chip.
 */
export function composeAutocomplete(before: string, after?: string): Promise<string> {
  return after === undefined
    ? invoke('compose_autocomplete', { before })
    : invoke('compose_autocomplete', { before, after });
}

/** Translates the message's body into the chosen language, cached. */
export function translateMessage(
  message: MessageRef,
  lang: TranslateLanguage,
): Promise<string> {
  return invoke('translate', { account: message.account, key: message.key, lang });
}

/** Pulls the message's smart-reply chips from the draft pool through the gate. */
export function chipsFor(message: MessageRef): Promise<ChipView[]> {
  return invoke('draft_chips', { account: message.account, key: message.key });
}

/** Summarises the message, cached: a hit answers without a model call. */
export function summariseMessage(message: MessageRef): Promise<string> {
  return invoke('summarise', { account: message.account, key: message.key });
}

/** The account's writing-style note, or null when none was built. */
export function accountStyleProfile(account: string): Promise<string | null> {
  return invoke('style_profile', { account });
}

/** Records the reader's dismissal of the message's chips. */
export function dismissChips(message: MessageRef): Promise<void> {
  return invoke('chips_dismiss', { account: message.account, key: message.key });
}

/**
 * Opens the reply composer on one accepted chip: the host builds the reply's
 * own prefill (recipients, subject, threading header, quoted original) and
 * publishes it with the drafted reply already the document's opening
 * paragraphs, so the composer hydrates from the request exactly as it does
 * from a reply's own dispatch.
 */
export function composeFromDraft(message: MessageRef, replyText: string): Promise<void> {
  return invoke('compose_from_draft', {
    account: message.account,
    key: message.key,
    replyText,
  });
}

/**
 * Subscribes to the draft-pool event: the host emits one per landed pool row
 * (drafted or skipped alike), and the handler receives the message it landed
 * for, so the reading pane can pull chips again when the row is its own.
 * Resolves the unlistener.
 */
export function onDraftPoolLanded(handler: (message: MessageRef) => void): Promise<() => void> {
  return listen<DraftPoolLanded>('draft_pool', (event) => {
    handler({ account: event.payload.account, key: event.payload.key });
  });
}
