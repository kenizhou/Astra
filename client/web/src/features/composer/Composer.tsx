// SPDX-License-Identifier: GPL-3.0-only
// The composer pane and its session: a New message action or an app start
// resume opens a composition with a minted id, the fields and the trimmed
// editor edit it, the idle autosave hands the whole draft to the core, and
// the status surface's own composition badges the save. Discard removes the
// stored copy and closes the pane locally; Close hands the whole current
// draft over as a final save that forgets the composition, then closes the
// pane too. Send submits the current document to the durable outbox; an
// accepted send clears the stored draft and closes, a refused one keeps the
// pane open with its words and says why, and neither its error nor its
// in-flight submit outlives its own composition.

import { useEffect, useRef, useState } from 'react';
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
    void draftResume(account.accountId, composition)
      .then((view) => {
        if (view === null) {
          return;
        }
        useComposerStore
          .getState()
          .hydrate(view, { id: account.accountId, label: account.displayName }, composition);
      })
      .catch((err: unknown) => {
        // A pull that cannot reach the host leaves the pane shut and the
        // session's one attempt spent; the next app start tries again.
        console.error('compose: the draft resume pull failed', err);
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

export function ComposerPane() {
  const open = useComposerStore((s) => s.open);
  const composition = useComposerStore((s) => s.composition);
  const fromLabel = useComposerStore((s) => s.fromLabel);
  const to = useComposerStore((s) => s.to);
  const cc = useComposerStore((s) => s.cc);
  const bcc = useComposerStore((s) => s.bcc);
  const subject = useComposerStore((s) => s.subject);
  const initialContent = useComposerStore((s) => s.editor.content);
  // A field's visibility is derived from its data: a toggle the user pressed
  // opens it, and a value the draft carries keeps it open, so a resumed
  // draft never hides the recipients it came with.
  const [ccToggled, setCcToggled] = useState(false);
  const [bccToggled, setBccToggled] = useState(false);
  const ccShown = ccToggled || cc !== '';
  const bccShown = bccToggled || bcc !== '';
  // The send's own state, local to the pane: an in-flight submit disarms its
  // button so one gesture cannot queue two sends, and a refused one says why.
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  // The send's epoch: each composition's submits settle only against
  // themselves, so one left in flight when the pane moved on can neither
  // close the next composition nor stick its error under it.
  const sendEpoch = useRef(0);

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

  // A composition inherits nothing from the one before it: the pane's own
  // state resets on the new, close, and hydrate paths, and so does the
  // send's error line and in-flight flag, with the epoch moved past
  // whatever was still in flight.
  useEffect(() => {
    sendEpoch.current += 1;
    setSending(false);
    setSendError(null);
  }, [composition]);

  if (!open || composition === null) {
    return null;
  }

  const discard = (): void => {
    void dispatch(intents.discardDraft(composition));
    useComposerStore.getState().closePane();
  };

  const close = (): void => {
    // The final save carries the whole current draft and asks the core to
    // forget the composition once it lands; the pane leaves now.
    const state = useComposerStore.getState();
    void dispatch(
      intents.saveDraft({
        composition,
        from: state.from,
        to: state.to,
        cc: state.cc,
        bcc: state.bcc,
        subject: state.subject,
        document: editorToDocument(state.editor),
        then_close: true,
      }),
    );
    useComposerStore.getState().closePane();
  };

  const send = (): void => {
    // The submit reads the pane's state at click time, so the document is
    // the one on screen: no flush window stands between the words and the
    // queue. A blank To would be refused before the queue with nothing
    // published, so the button never offers it.
    const state = useComposerStore.getState();
    if (state.to.trim() === '') {
      return;
    }
    setSendError(null);
    setSending(true);
    const run = ++sendEpoch.current;
    void dispatch(
      intents.submitRichMail({
        from: state.from,
        to: state.to,
        cc: state.cc,
        bcc: state.bcc,
        subject: state.subject,
        document: editorToDocument(state.editor),
        blobs: [],
      }),
    )
      .then(() => {
        if (sendEpoch.current !== run) {
          // The composition this send belonged to is gone; its settlement
          // must not touch whatever the pane shows now.
          return;
        }
        // The submit carries no composition id, so the stored draft the
        // autosave left behind goes with an explicit discard.
        void dispatch(intents.discardDraft(composition));
        useComposerStore.getState().closePane();
      })
      .catch((err: unknown) => {
        if (sendEpoch.current !== run) {
          return;
        }
        setSending(false);
        setSendError(sendErrorMessage(err));
      });
  };
  const sendGated = to.trim() === '';

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
            onClick={send}
            disabled={sendGated || sending}
            title={sendGated ? 'Add a recipient first' : 'Send'}
            className="rounded-[var(--radius-xs)] bg-primary px-2 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-50"
          >
            Send
          </button>
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
      {sendError !== null && (
        <p
          role="alert"
          className="shrink-0 border-b border-border bg-background px-3 py-1.5 text-xs text-destructive"
        >
          {sendError}
        </p>
      )}
      <ComposerFields
        fromLabel={fromLabel}
        to={to}
        cc={cc}
        bcc={bcc}
        subject={subject}
        ccShown={ccShown}
        bccShown={bccShown}
        onField={(field, value) => useComposerStore.getState().setField(field, value)}
        onToggleCc={() => setCcToggled((toggled) => !toggled)}
        onToggleBcc={() => setBccToggled((toggled) => !toggled)}
      />
      <Editor
        key={composition}
        initialContent={initialContent}
        onChange={(doc) => useComposerStore.getState().setEditorContent(doc.content ?? [])}
      />
    </section>
  );
}
