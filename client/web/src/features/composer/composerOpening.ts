// SPDX-License-Identifier: GPL-3.0-only
// The composer's opening rules as pure helpers: the ids a composition is
// known by, the caret predicate an open runs, the quote ordering the
// quote-style setting seeds, the prefill a mailto link opens, and the
// handover fingerprint. Split from composerStore, which holds the session
// state, so each file keeps one concern under the line cap; the store
// re-exports every name, so importers keep the one path they had.

import type { ComposeKind, ComposerDocument, CompositionId } from '@/services/ipc';

import {
  documentFromPlainText,
  documentToEditor,
  editorToDocument,
  type EditorDocument,
} from './documentWire';
import { normaliseRecipients } from './recipientTokens';

/** Mints one composition's id: a UUID, unique across restarts. */
export function mintCompositionId(): CompositionId {
  return crypto.randomUUID();
}

/**
 * The one predicate the composer's opening caret runs on: in the body when
 * the composer was already addressed (a reply, a forward's quoted original,
 * a resumed draft, a prefilled link), in To otherwise, where the work starts.
 */
export function opensInBody(request: CompositionRequest | null, to: string): boolean {
  return request !== null || to.trim() !== '';
}

/** The account a composition sends from. */
export interface SendAccount {
  id: string;
  label: string;
}

/** The reply or forward a composition is, when a compose request opened it. */
export interface CompositionRequest {
  kind: ComposeKind;
  inReplyTo: string | null;
}

/** The fields a mailto link prefilled a plain composition with: the parser's
 *  five, nothing else; every field may be empty. */
export interface ComposePrefill {
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  body: string;
}

/**
 * The opening state a prefill seeds: the recipient fields normalised the way
 * every open normalises them (each supplied address a finished pill, nothing
 * in progress), and the body as the document it stands for, so a prefilled
 * composition opens exactly as a hand-typed one would read.
 */
export function prefillFields(prefill: ComposePrefill): {
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  editor: EditorDocument;
} {
  return {
    to: normaliseRecipients(prefill.to),
    cc: normaliseRecipients(prefill.cc),
    bcc: normaliseRecipients(prefill.bcc),
    subject: prefill.subject,
    editor: documentToEditor(documentFromPlainText(prefill.body)),
  };
}

/**
 * Orders a request's quote blocks per the quote-style setting: `below`
 * leaves the document as the core built it (the reply's words above the
 * quoted original), otherwise each quote moves to the top so the reply is
 * typed under it. The setting seeds only the opening order: the document
 * the composer then edits and saves is the per-message truth.
 */
export function quoteOrdered(document: ComposerDocument, below: boolean): ComposerDocument {
  if (below) {
    return document;
  }
  const quotes = document.blocks.filter((block) => 'Quote' in block);
  if (quotes.length === 0) {
    return document;
  }
  const rest = document.blocks.filter((block) => !('Quote' in block));
  return { ...document, blocks: [...quotes, ...rest] };
}

/** What a handover would carry, as comparable text. */
export function fingerprintOf(state: {
  from: string;
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  editor: EditorDocument;
}): string {
  return JSON.stringify([
    state.from,
    state.to,
    state.cc,
    state.bcc,
    state.subject,
    editorToDocument(state.editor),
  ]);
}
