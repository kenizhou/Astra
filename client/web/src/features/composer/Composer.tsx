// SPDX-License-Identifier: GPL-3.0-only
// The composer pane and its session: a New message action or an app start
// resume opens a composition with a minted id, the fields and the trimmed
// editor edit it, the idle autosave hands the whole draft to the core, and
// the status surface's own composition badges the save. Discard removes the
// stored copy and closes the pane locally; Close asks the core to save the
// held content and forget the composition, then closes the pane too.

import { useEffect, useState } from 'react';
import { create } from 'zustand';
import type { JSONContent } from '@tiptap/react';

import { dispatch, draftResume, intents, type CompositionId, type ResumeView } from '@/services/ipc';
import { isTauri } from '@/events/surfaceEvents';
import { useAccountStore } from '@/stores/accountStore';
import { useDraftStore } from '@/stores/draftStore';

import { ComposerFields } from './ComposerFields';
import { Editor } from './Editor';
import {
  documentFromPlainText,
  documentToEditor,
  editorToDocument,
  type EditorDocument,
} from './documentWire';
import { useAutosave } from './useAutosave';

/** Mints one composition's id: a UUID, unique across restarts. */
export function mintCompositionId(): CompositionId {
  return crypto.randomUUID();
}

/** The account a composition sends from. */
interface SendAccount {
  id: string;
  label: string;
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
  newMessage: (account: SendAccount) => void;
  /** Opens a resumed draft under the composition the resume pull named. */
  hydrate: (view: ResumeView, account: SendAccount, composition: CompositionId) => void;
  setField: (field: 'to' | 'cc' | 'bcc' | 'subject', value: string) => void;
  setEditorContent: (content: JSONContent[]) => void;
  closePane: () => void;
}

const emptyEditor = (): EditorDocument => documentToEditor(documentFromPlainText(''));

/** What a handover would carry, as comparable text. */
function fingerprintOf(state: {
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

  hydrate: (view, account, composition) =>
    set(() => {
      const editor = documentToEditor(documentFromPlainText(view.body_text));
      const state = {
        open: true,
        composition,
        from: account.id,
        fromLabel: account.label,
        to: view.to,
        cc: view.cc,
        bcc: view.bcc,
        subject: view.subject,
        editor,
      };
      return { ...state, savedFingerprint: fingerprintOf(state) };
    }),

  setField: (field, value) => set({ [field]: value }),

  setEditorContent: (content) =>
    set((state) => ({ editor: { ...state.editor, content } })),

  closePane: () => set({ ...closedState }),
}));

/**
 * The app start resume: once, when the first account is known, the stored
 * draft of that account opens into a freshly minted composition, so its
 * saves supersede the copy it came from. No draft leaves the pane shut.
 */
export function useDraftResumeAtStart(): void {
  const accounts = useAccountStore((s) => s.accounts);

  useEffect(() => {
    const store = useComposerStore.getState();
    const account = accounts[0];
    if (store.resumeAttempted || !isTauri || account === undefined) {
      return;
    }
    const composition = mintCompositionId();
    useComposerStore.setState({ resumeAttempted: true });
    void draftResume(account.accountId, composition).then((view) => {
      if (view === null) {
        return;
      }
      useComposerStore
        .getState()
        .hydrate(view, { id: account.accountId, label: account.displayName }, composition);
    });
  }, [accounts]);
}

/** The floating New message action, anchored over the shell's corner. */
export function NewMessageAction({ disabled }: { disabled: boolean }) {
  const open = (): void => {
    const account = useAccountStore.getState().accounts[0];
    if (account === undefined) {
      return;
    }
    useComposerStore.getState().newMessage({ id: account.accountId, label: account.displayName });
  };

  return (
    <button
      type="button"
      aria-label="New message"
      onClick={open}
      disabled={disabled}
      title={disabled ? 'Add an account first' : 'New message'}
      className="absolute right-3 bottom-[48px] rounded-[var(--radius-sm)] bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground shadow-lg transition-colors hover:bg-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-50"
    >
      New message
    </button>
  );
}

/** The save badge: the status surface's word for this composition alone. */
function SaveBadge({ composition }: { composition: CompositionId }) {
  const snapshot = useDraftStore((s) => s.snapshot);
  if (snapshot === null || snapshot.composition !== composition) {
    return null;
  }
  if (snapshot.state === 'saving') {
    return <span className="text-xs text-muted-foreground">Saving draft</span>;
  }
  const time = new Date(snapshot.saved_at).toISOString().slice(11, 16);
  return <span className="text-xs text-muted-foreground">{`Saved ${time} UTC`}</span>;
}

export function ComposerPane() {
  const open = useComposerStore((s) => s.open);
  const composition = useComposerStore((s) => s.composition);
  const fromLabel = useComposerStore((s) => s.fromLabel);
  const to = useComposerStore((s) => s.to);
  const cc = useComposerStore((s) => s.cc);
  const bcc = useComposerStore((s) => s.bcc);
  const subject = useComposerStore((s) => s.subject);
  const initialContent = useComposerStore((s) => s.editor.content);
  const [ccShown, setCcShown] = useState(() => cc !== '');
  const [bccShown, setBccShown] = useState(() => bcc !== '');

  const autosave = useAutosave({
    isDirty: () => {
      const state = useComposerStore.getState();
      return fingerprintOf(state) !== state.savedFingerprint;
    },
    onIdle: () => {
      const state = useComposerStore.getState();
      if (!state.open || state.composition === null) {
        return;
      }
      void dispatch(
        intents.saveDraft({
          composition: state.composition,
          from: state.from,
          to: state.to,
          cc: state.cc,
          bcc: state.bcc,
          subject: state.subject,
          document: editorToDocument(state.editor),
          then_close: false,
        }),
      );
      useComposerStore.setState({ savedFingerprint: fingerprintOf(state) });
    },
  });

  // The pane unmounts with each close (it renders nothing when shut), so
  // the subscription arms the autosave for exactly the open composition.
  useEffect(() => {
    if (!open) {
      return;
    }
    return useComposerStore.subscribe(() => autosave.noteEdit());
  }, [open, autosave]);

  if (!open || composition === null) {
    return null;
  }

  const discard = (): void => {
    void dispatch(intents.discardDraft(composition));
    useComposerStore.getState().closePane();
  };

  const close = (): void => {
    void dispatch(intents.closeDraft(composition));
    useComposerStore.getState().closePane();
  };

  return (
    <section
      aria-label="Composer"
      className="flex w-[520px] shrink-0 flex-col overflow-hidden border-l border-border bg-background"
    >
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between gap-2 border-b border-border bg-surface px-3">
        <SaveBadge composition={composition} />
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={discard}
            className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Discard
          </button>
          <button
            type="button"
            aria-label="Close composer"
            onClick={close}
            className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Close
          </button>
        </div>
      </div>
      <ComposerFields
        fromLabel={fromLabel}
        to={to}
        cc={cc}
        bcc={bcc}
        subject={subject}
        ccShown={ccShown}
        bccShown={bccShown}
        onField={(field, value) => useComposerStore.getState().setField(field, value)}
        onToggleCc={() => setCcShown((shown) => !shown)}
        onToggleBcc={() => setBccShown((shown) => !shown)}
      />
      <Editor
        key={composition}
        initialContent={initialContent}
        onChange={(doc) => useComposerStore.getState().setEditorContent(doc.content ?? [])}
      />
    </section>
  );
}
