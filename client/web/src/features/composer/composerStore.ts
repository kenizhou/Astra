// SPDX-License-Identifier: GPL-3.0-only
// The composer session's store: the composition the pane holds, its fields,
// its split document, and the compose request it answers, plus the send's
// own state and submit path so the ribbon's Send and the pane's Send are one
// gesture, and the live editor handle so an insert from outside the pane (the
// ribbon's snippet picker) lands at the current selection. Split from
// Composer.tsx so the pane renders and the store holds, one concern each,
// both under the line cap. A composition inherits nothing from the one
// before it: every opening path resets the whole state, and the pane's own
// local state moves with the composition id. A request's quoted original
// opens ordered by the quote-style setting, the setting's one seed into
// the document the per-message truth then owns.

import { create } from 'zustand';
import type { Editor as CoreEditor } from '@tiptap/core';
import type { JSONContent } from '@tiptap/react';

import type {
  ComposeKind,
  ComposeRequestSnapshot,
  CompositionId,
  ComposerDocument,
  Intent,
  ResumeView,
} from '@/services/ipc';
import { dispatch, intents } from '@/services/ipc';
import { useComposeRequestStore } from '@/stores/composeRequestStore';
import { useSettingsStore } from '@/features/settings/settingsStore';

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
  /** Whether a send submit is in flight; one gesture cannot queue two sends. */
  sending: boolean;
  /** Why the last submit was refused, in words the pane can show. */
  sendError: string | null;
  newMessage: (account: SendAccount) => void;
  /** Opens a resumed draft under the composition the resume pull named. */
  hydrate: (view: ResumeView, account: SendAccount, composition: CompositionId) => void;
  /** Opens the composition a compose request prefilled, minting its id. */
  openRequest: (snapshot: ComposeRequestSnapshot, fromLabel: string) => void;
  setField: (field: 'to' | 'cc' | 'bcc' | 'subject', value: string) => void;
  setEditorContent: (content: JSONContent[]) => void;
  /** Submits the current document to the durable outbox, the send's one path. */
  submit: () => void;
  closePane: () => void;
}

const emptyEditor = (): EditorDocument => documentToEditor(documentFromPlainText(''));

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
  sending: false,
  sendError: null,
};

/** A refused send in words the pane can show, whatever the host handed back. */
function sendErrorMessage(err: unknown): string {
  if (err instanceof Error) {
    return err.message;
  }
  if (typeof err === 'string') {
    return err;
  }
  return 'The message was not sent';
}

/** A reply whose original arrived with no Message-ID: nothing to thread under. */
const REPLY_WITHOUT_ORIGIN = 'The original message has no Message-ID to reply under';

/**
 * The submit intent the composition's origin calls for. A plain composition
 * sends as a fresh mail; one a request opened sends through its own threaded
 * submit, the reply under its original, the forward answering no one. The
 * threaded wires carry no bcc and the forward's no cc either, so those fields
 * are the plain send's alone.
 */
function submitIntent(state: {
  request: CompositionRequest | null;
  from: string;
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  editor: EditorDocument;
}): Intent {
  const document = editorToDocument(state.editor);
  if (state.request === null) {
    return intents.submitRichMail({
      from: state.from,
      to: state.to,
      cc: state.cc,
      bcc: state.bcc,
      subject: state.subject,
      document,
      blobs: [],
    });
  }
  if (state.request.kind === 'forward') {
    return intents.submitRichForward({
      from: state.from,
      to: state.to,
      subject: state.subject,
      document,
      blobs: [],
    });
  }
  return intents.submitRichReply({
    from: state.from,
    to: state.to,
    cc: state.cc,
    in_reply_to: state.request.inReplyTo ?? '',
    subject: state.subject,
    document,
    blobs: [],
  });
}

/** The live editor the open composition types into; null while none stands. */
let activeEditor: CoreEditor | null = null;

/** Records the editor the composer pane just mounted or unmounted. */
export function registerComposerEditor(editor: CoreEditor | null): void {
  activeEditor = editor;
}

/** The live composer editor, for inserts from outside the pane. */
export function composerEditor(): CoreEditor | null {
  return activeEditor;
}

export const useComposerStore = create<ComposerStoreState>((set, get) => ({
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
      editor: documentToEditor(
        quoteOrdered(snapshot.document, useSettingsStore.getState().settings.quote_below),
      ),
      request: { kind: snapshot.kind, inReplyTo: snapshot.in_reply_to ?? null },
      // Nothing is stored yet, so the prefilled composition counts as dirty
      // from birth: the idle autosave gives it its first save unedited.
      savedFingerprint: null,
    }),

  setField: (field, value) => set({ [field]: value }),

  setEditorContent: (content) =>
    set((state) => ({ editor: { ...state.editor, content } })),

  submit: () => {
    // The submit reads the store at click time, so the document is the one
    // on screen. A blank To would be refused before the queue with nothing
    // published, so the button never offers it. A reply whose original
    // carried no Message-ID is refused here too: the core would reject the
    // blank threading header after the queue, a dead end with the words
    // already spent, so the surface says it and keeps them.
    const state = get();
    const composition = state.composition;
    if (!state.open || composition === null || state.sending) {
      return;
    }
    if (state.to.trim() === '') {
      return;
    }
    if (
      state.request !== null &&
      state.request.kind !== 'forward' &&
      state.request.inReplyTo === null
    ) {
      set({ sendError: REPLY_WITHOUT_ORIGIN });
      return;
    }
    set({ sendError: null, sending: true });
    void dispatch(submitIntent(state)).then(
      () => {
        // A settlement lands only against its own composition: one left in
        // flight when the pane moved on can neither close the next
        // composition nor stick its error under it.
        if (get().composition !== composition) {
          return;
        }
        // The submit carries no composition id, so the stored draft the
        // autosave left behind goes with an explicit discard.
        void dispatch(intents.discardDraft(composition));
        get().closePane();
      },
      (err: unknown) => {
        if (get().composition !== composition) {
          return;
        }
        set({ sending: false, sendError: sendErrorMessage(err) });
      },
    );
  },

  closePane: () => {
    // The composer's leaving dismisses the request it opened, so nothing
    // stays pending behind a shut pane.
    useComposeRequestStore.getState().dismiss();
    set({ ...closedState });
  },
}));
