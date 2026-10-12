// SPDX-License-Identifier: GPL-3.0-only
// The composer session's store: the composition the pane holds, its fields,
// its split document, and the compose request it answers. Split from
// Composer.tsx so the pane renders and the store holds, one concern each,
// both under the line cap. A composition inherits nothing from the one
// before it: every opening path resets the whole state, and the pane's own
// local state moves with the composition id.

import { create } from 'zustand';
import type { JSONContent } from '@tiptap/react';

import type {
  ComposeKind,
  ComposeRequestSnapshot,
  CompositionId,
  ResumeView,
} from '@/services/ipc';
import { useComposeRequestStore } from '@/stores/composeRequestStore';

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
 * a resumed draft), in To otherwise, where the work starts.
 */
export function opensInBody(request: CompositionRequest | null, to: string): boolean {
  return request !== null || to.trim() !== '';
}

/** The account a composition sends from. */
interface SendAccount {
  id: string;
  label: string;
}

/** The reply or forward a composition is, when a compose request opened it. */
export interface CompositionRequest {
  kind: ComposeKind;
  inReplyTo: string | null;
}

interface ComposerStoreState {
  /** Whether the composer pane is up. */
  open: boolean;
  /** Whether the app start resume has been attempted, once per session. */
  resumeAttempted: boolean;
  composition: CompositionId | null;
  from: string;
  fromLabel: string;
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  /** The document split for the editor, retained blocks and attachments. */
  editor: EditorDocument;
  /** What the last handover carried; equal content costs no dispatch. */
  savedFingerprint: string | null;
  /** The compose request this composition answers; null for a plain one. */
  request: CompositionRequest | null;
  newMessage: (account: SendAccount) => void;
  /** Opens a resumed draft under the composition the resume pull named. */
  hydrate: (view: ResumeView, account: SendAccount, composition: CompositionId) => void;
  /** Opens the composition a compose request prefilled, minting its id. */
  openRequest: (snapshot: ComposeRequestSnapshot, fromLabel: string) => void;
  setField: (field: 'to' | 'cc' | 'bcc' | 'subject', value: string) => void;
  setEditorContent: (content: JSONContent[]) => void;
  closePane: () => void;
}

const emptyEditor = (): EditorDocument => documentToEditor(documentFromPlainText(''));

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

const closedState = {
  open: false,
  composition: null,
  from: '',
  fromLabel: '',
  to: '',
  cc: '',
  bcc: '',
  subject: '',
  editor: emptyEditor(),
  savedFingerprint: null,
  request: null,
};

export const useComposerStore = create<ComposerStoreState>((set) => ({
  ...closedState,
  resumeAttempted: false,

  newMessage: (account) =>
    set({
      ...closedState,
      open: true,
      composition: mintCompositionId(),
      from: account.id,
      fromLabel: account.label,
    }),

  hydrate: (view, account, composition) => {
    // A resumed composition is a plain one: whatever request the pane held,
    // or the store still has pending, is spent here so the resumed send
    // rides the plain submit with the resumed fields. The recipient fields
    // normalise their opening value at seed time, so every supplied address
    // is a finished pill, nothing sits in progress, and the fingerprint
    // compares against the normalised value.
    useComposeRequestStore.getState().dismiss();
    set(() => {
      const editor = documentToEditor(documentFromPlainText(view.body_text));
      const state = {
        open: true,
        composition,
        from: account.id,
        fromLabel: account.label,
        to: normaliseRecipients(view.to),
        cc: normaliseRecipients(view.cc),
        bcc: normaliseRecipients(view.bcc),
        subject: view.subject,
        editor,
      };
      return { ...state, request: null, savedFingerprint: fingerprintOf(state) };
    });
  },

  openRequest: (snapshot, fromLabel) =>
    set({
      ...closedState,
      open: true,
      composition: mintCompositionId(),
      from: snapshot.from,
      fromLabel,
      to: normaliseRecipients(snapshot.to),
      cc: normaliseRecipients(snapshot.cc),
      subject: snapshot.subject,
      editor: documentToEditor(snapshot.document),
      request: { kind: snapshot.kind, inReplyTo: snapshot.in_reply_to ?? null },
      // Nothing is stored yet, so the prefilled composition counts as dirty
      // from birth: the idle autosave gives it its first save unedited.
      savedFingerprint: null,
    }),

  setField: (field, value) => set({ [field]: value }),

  setEditorContent: (content) =>
    set((state) => ({ editor: { ...state.editor, content } })),

  closePane: () => {
    // The composer's leaving dismisses the request it opened, so nothing
    // stays pending behind a shut pane.
    useComposeRequestStore.getState().dismiss();
    set({ ...closedState });
  },
}));
