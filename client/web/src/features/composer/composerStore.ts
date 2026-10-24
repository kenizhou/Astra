// SPDX-License-Identifier: GPL-3.0-only
// The composer session's store: the composition the pane holds, its fields,
// its split document, and the compose request it answers, plus the send's
// own state and submit path so the ribbon's Send and the pane's Send are one
// gesture, and the live editor handle so an insert from outside the pane (the
// ribbon's snippet picker) lands at the current selection. One store lives
// per window: the main window's drives the docked quick-reply strip, a
// pop-out composer window's drives its own, and the pop-out's opening paths
// take the id the main window minted rather than minting anew. Split from
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
  ComposeRequestSnapshot,
  CompositionId,
  ResumeView,
  Signature,
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
import {
  attachFile,
  inlineRef,
  stripInlineRefs,
  type StagedAttachment,
} from './composerStaging';
import {
  fingerprintOf,
  mintCompositionId,
  prefillFields,
  quoteOrdered,
  type ComposePrefill,
  type CompositionRequest,
  type SendAccount,
} from './composerOpening';
import { normaliseRecipients } from './recipientTokens';
import { REPLY_WITHOUT_ORIGIN, SHARES_NOT_ATTACHED, sendErrorMessage, submitIntent } from './composerSubmit';
import {
  seedSignatureBlocks,
  signatureForAccount,
  swapSeededSignature,
} from './signatureSeeding';
import { degradingLinkTargets, degradationSentence } from './sendLinks';
import type { SharedFile } from './shareOpen';

// The opening rules live in composerOpening; re-exported here so every
// importer keeps the one path it always had.
export {
  fingerprintOf,
  mintCompositionId,
  opensInBody,
  prefillFields,
  quoteOrdered,
} from './composerOpening';
export type {
  ComposePrefill,
  CompositionRequest,
  MailtoOpen,
  SendAccount,
} from './composerOpening';

interface ComposerStoreState {
  /** Whether the composer pane is up. */
  open: boolean;
  /** Whether the app start resume has been attempted, once per session. */
  resumeAttempted: boolean;
  /**
   * Whether the next landed compose request opens the inline quick-reply
   * strip rather than a pop-out window: the strip's expanding click sets it,
   * the request's opening spends it.
   */
  inlineExpected: boolean;
  composition: CompositionId | null;
  from: string;
  fromLabel: string;
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  /** The document split for the editor, retained blocks and attachments. */
  editor: EditorDocument;
  /** The composer's own staged files, keyed by the row's blob handle. */
  files: Record<string, File>;
  /**
   * The OS share hand-off's files, pending: their names render, their
   * paths never do, and their bytes stage only when the read seam ships.
   */
  pendingShares: SharedFile[];
  /** Dropped pictures awaiting the inline-or-file answer, newest drop. */
  pendingImages: File[];
  /** What the last handover carried; equal content costs no dispatch. */
  savedFingerprint: string | null;
  /** The compose request this composition answers; null for a plain one. */
  request: CompositionRequest | null;
  /**
   * The signature the composition opened seeded, the baseline a From
   * change swaps against; null when the open seeded nothing.
   */
  seeded: Signature | null;
  /**
   * The send-time link degradation sentence, named before the submit and
   * rendered under the toolbar; null when every target stands. Never a
   * blocked send: the core's degradation stays the send's floor.
   */
  linkWarning: string | null;
  /** Whether a send submit is in flight; one gesture cannot queue two sends. */
  sending: boolean;
  /** Why the last submit was refused, in words the pane can show. */
  sendError: string | null;
  /**
   * Opens a plain composition. `composition` is the caller-minted id of the
   * pop-out window the composition lives in; absent, one is minted here.
   * `prefill`, the shape a mailto link fills, seeds the fields and the body;
   * the composition stays request-free and sends as plain mail.
   */
  newMessage: (
    account: SendAccount,
    composition?: CompositionId,
    prefill?: ComposePrefill,
  ) => void;
  /** Opens a resumed draft under the composition the resume pull named. */
  hydrate: (view: ResumeView, account: SendAccount, composition: CompositionId) => void;
  /**
   * Opens the composition a compose request prefilled. `composition` is the
   * caller-minted id of the pop-out window; absent, one is minted here.
   */
  openRequest: (
    snapshot: ComposeRequestSnapshot,
    fromLabel: string,
    composition?: CompositionId,
  ) => void;
  /** Flags the next landed compose request for the inline quick-reply strip. */
  expectInlineReply: () => void;
  setField: (field: 'to' | 'cc' | 'bcc' | 'subject', value: string) => void;
  setEditorContent: (content: JSONContent[]) => void;
  /**
   * Changes the account the composition sends from: the fields move and a
   * seeded signature the person has not touched swaps to the new
   * account's own; an edited signature stays.
   */
  changeFrom: (account: SendAccount) => void;
  /** Stands each picked file as a removable document row. */
  attachFiles: (files: File[]) => void;
  /** Seeds the share hand-off's files as the composition's pending set. */
  seedShares: (files: SharedFile[]) => void;
  /** Drops one pending share by its name. */
  removeShare: (share: { name: string; path: string }) => void;
  /** Takes one dropped batch: documents attach, pictures ask first. */
  dropFiles: (files: File[]) => void;
  /** Answers the dropped pictures' ask: inline content or a file row. */
  answerPendingImages: (inline: boolean) => void;
  /** Stages pasted pictures inline, exactly as a drop's inline answer does. */
  pasteImages: (files: File[]) => void;
  /** Removes one attachment row, its staged bytes, and its references. */
  removeAttachment: (id: string) => void;
  /** Submits the current document to the durable outbox, the send's one path. */
  submit: () => void;
  closePane: () => void;
}

const emptyEditor = (): EditorDocument => documentToEditor(documentFromPlainText(''));

const closedState = {
  open: false,
  inlineExpected: false,
  composition: null,
  from: '',
  fromLabel: '',
  to: '',
  cc: '',
  bcc: '',
  subject: '',
  editor: emptyEditor(),
  files: {},
  pendingShares: [],
  pendingImages: [],
  savedFingerprint: null,
  request: null,
  seeded: null,
  linkWarning: null,
  sending: false,
  sendError: null,
};

/** The submit path's own helpers live in composerSubmit; the store holds
 *  the gesture: one click, one send, settled against its own composition. */

/** The live editor the open composition types into; null while none stands. */
let activeEditor: CoreEditor | null = null;

/** Folds staged attachments into the state: their rows onto the document,
 *  their files behind the minted handles. */
function stage(
  state: { editor: EditorDocument; files: Record<string, File> },
  staged: StagedAttachment[],
): { editor: EditorDocument; files: Record<string, File> } {
  if (staged.length === 0) {
    return state;
  }
  const files = { ...state.files };
  for (const entry of staged) {
    if (entry.file !== undefined && entry.row.blob !== null) {
      files[entry.row.blob] = entry.file;
    }
  }
  return {
    ...state,
    editor: {
      ...state.editor,
      attachments: [...state.editor.attachments, ...staged.map((entry) => entry.row)],
    },
    files,
  };
}

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

  newMessage: (account, composition, prefill) => {
    // A plain composition seeds the account's new-mail signature at the
    // top of the document, so the person's words grow under it.
    const seeded = signatureForAccount(account.id, false);
    const plain = documentFromPlainText(prefill?.body ?? '');
    const document =
      seeded === null ? plain : { ...plain, blocks: seedSignatureBlocks(plain.blocks, seeded) };
    set({
      ...closedState,
      open: true,
      composition: composition ?? mintCompositionId(),
      from: account.id,
      fromLabel: account.label,
      seeded,
      // A mailto link's prefill seeds the same normalised fields a hand
      // typed open would hold; no prefill crosses, the blank state stands.
      ...(prefill === undefined ? {} : prefillFields(prefill)),
      editor: documentToEditor(document),
    });
  },

  hydrate: (view, account, composition) => {
    // A resumed composition is a plain one: whatever request the pane held,
    // or the store still has pending, is spent here so the resumed send
    // rides the plain submit with the resumed fields. The recipient fields
    // normalise their opening value at seed time, so every supplied address
    // is a finished pill, nothing sits in progress, and the fingerprint
    // compares against the normalised value. A resumed draft owns the
    // document it comes with, so nothing seeds here.
    useComposeRequestStore.getState().dismiss();
    set(() => {
      const editor = documentToEditor(documentFromPlainText(view.body_text));
      const state = {
        ...closedState,
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

  openRequest: (snapshot, fromLabel, composition) => {
    // A request seeds the account's reply signature beneath the quoted
    // original, BEFORE the quote ordering moves anything, so the pinned
    // pipeline holds: the signature stays below the typed words whatever
    // the quote-style setting does to the quote itself.
    const seeded = signatureForAccount(snapshot.from, true);
    const seededDocument =
      seeded === null
        ? snapshot.document
        : { ...snapshot.document, blocks: seedSignatureBlocks(snapshot.document.blocks, seeded) };
    set({
      ...closedState,
      open: true,
      composition: composition ?? mintCompositionId(),
      from: snapshot.from,
      fromLabel,
      to: normaliseRecipients(snapshot.to),
      cc: normaliseRecipients(snapshot.cc),
      subject: snapshot.subject,
      editor: documentToEditor(
        quoteOrdered(seededDocument, useSettingsStore.getState().settings.quote_below),
      ),
      request: { kind: snapshot.kind, inReplyTo: snapshot.in_reply_to ?? null },
      seeded,
      // Nothing is stored yet, so the prefilled composition counts as dirty
      // from birth: the idle autosave gives it its first save unedited.
      savedFingerprint: null,
    });
  },

  expectInlineReply: () => set({ inlineExpected: true }),

  setField: (field, value) => set({ [field]: value }),

  setEditorContent: (content) =>
    set((state) => ({ editor: { ...state.editor, content } })),

  changeFrom: (account) =>
    set((state) => {
      // The swap honours the composition's own kind: a request seeds the
      // reply slot, a plain composition the new-mail one. A signature the
      // person edited no longer equals the seeded baseline and stays, and
      // the baseline stays stale with it, so later changes keep leaving the
      // person's words alone.
      const incoming = signatureForAccount(account.id, state.request !== null);
      const editor = swapSeededSignature(state.editor, state.seeded, incoming);
      const swapped = editor !== state.editor;
      return {
        from: account.id,
        fromLabel: account.label,
        editor,
        seeded: swapped ? incoming : state.seeded,
      };
    }),

  attachFiles: (files) =>
    set((state) => stage(state, files.map((file) => attachFile(file, false)))),

  seedShares: (files) => set({ pendingShares: files }),

  removeShare: (share) =>
    set((state) => ({
      pendingShares: state.pendingShares.filter(
        (file) => file.name !== share.name || file.path !== share.path,
      ),
    })),

  dropFiles: (files) => {
    const pictures = files.filter((file) => file.type.startsWith('image/'));
    const documents = files.filter((file) => !file.type.startsWith('image/'));
    set((state) => ({
      ...stage(state, documents.map((file) => attachFile(file, false))),
      // One ask covers the whole drop: every picture in it takes the same
      // answer, so a mixed batch is one decision, not one per file.
      pendingImages: pictures,
    }));
  },

  answerPendingImages: (inline) =>
    set((state) => {
      let next = stage(
        state,
        state.pendingImages.map((file) => attachFile(file, inline)),
      );
      const added = next.editor.attachments.slice(state.editor.attachments.length);
      for (const row of added) {
        if (inline && typeof row.disposition !== 'string') {
          next = { ...next, editor: inlineRef(next.editor, row.id, row.file_name) };
        }
      }
      return { ...next, pendingImages: [] };
    }),

  pasteImages: (files) =>
    set((state) => {
      // A paste into the editor carries the inline intent already, so it
      // takes the drop's inline answer directly: the inline disposition,
      // the cid binding, the removable row. No ask stands.
      let next = stage(state, files.map((file) => attachFile(file, true)));
      const added = next.editor.attachments.slice(state.editor.attachments.length);
      for (const row of added) {
        if (typeof row.disposition !== 'string') {
          next = { ...next, editor: inlineRef(next.editor, row.id, row.file_name) };
        }
      }
      return next;
    }),

  removeAttachment: (id) =>
    set((state) => {
      const row = state.editor.attachments.find((attachment) => attachment.id === id);
      if (row === undefined) {
        return state;
      }
      const files = { ...state.files };
      if (row.blob !== null) {
        delete files[row.blob];
      }
      const editor = stripInlineRefs(
        { ...state.editor, attachments: state.editor.attachments.filter((attachment) => attachment.id !== id) },
        id,
      );
      return { ...state, files, editor };
    }),

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
    // A pending share stages no bytes (the read seam is the named
    // remainder), and a send that dropped the named file silently would
    // be a lie, so the send refuses with the sentence until the last one
    // is removed.
    if (state.pendingShares.length > 0) {
      set({ sendError: SHARES_NOT_ATTACHED });
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
    // The send-time link check runs before the submit: the document's
    // targets are validated against the core's allowlist and any that
    // degrade are named in the sentence under the toolbar. The send is
    // never blocked by it; the core's degradation to plain text stays the
    // send's own floor.
    const warning = degradationSentence(degradingLinkTargets(editorToDocument(state.editor)));
    set({ linkWarning: warning });
    // A settlement lands only against its own composition: one left in
    // flight when the pane moved on can neither close the next
    // composition nor stick its error under it.
    const settle = (err: unknown): void => {
      if (get().composition !== composition) {
        return;
      }
      set({ sending: false, sendError: sendErrorMessage(err) });
    };
    void submitIntent(state).then(
      (intent) =>
        void dispatch(intent).then(
          () => {
            // The submit carries no composition id, so the stored draft the
            // autosave left behind goes with an explicit discard.
            void dispatch(intents.discardDraft(composition));
            get().closePane();
          },
          settle,
        ),
      settle,
    );
  },

  closePane: () => {
    // The composer's leaving dismisses the request it opened, so nothing
    // stays pending behind a shut pane.
    useComposeRequestStore.getState().dismiss();
    set({ ...closedState });
  },
}));
