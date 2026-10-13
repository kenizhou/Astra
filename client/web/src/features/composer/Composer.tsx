// SPDX-License-Identifier: GPL-3.0-only
// The composer pane and its session: a New message action, an app start
// resume, or a landed compose request opens a composition with a minted id,
// the fields and the trimmed editor edit it, the idle autosave hands the
// whole draft to the core, and the status surface's own composition badges
// the save. A composition a request opened carries its kind and threading
// header until the pane leaves, so its saves keep the quote and its send
// rides the threaded submit. Discard removes the stored copy and closes the
// pane locally; Close hands the whole current draft over as a final save
// that forgets the composition, then closes the pane too; every close
// dismisses the request the composition came from. Send submits the current
// document to the durable outbox through the store's one submit path (the
// ribbon's Send rides the same); an accepted send clears the stored draft
// and closes, a refused one keeps the pane open with its words and says
// why, and neither its error nor its in-flight submit outlives its own
// composition. The session's state itself lives in composerStore, which
// also holds the live editor handle the ribbon's snippet picker inserts
// through. The pane's own toolbar carries the same picker, placing the
// chosen body at the editor's current selection.

import { useEffect, useRef, useState } from 'react';
import type { Editor as CoreEditor } from '@tiptap/core';

import {
  dispatch,
  draftResume,
  intents,
  type CompositionId,
} from '@/services/ipc';
import { isTauri } from '@/events/surfaceEvents';
import { useAccountStore } from '@/stores/accountStore';
import { useDraftStore } from '@/stores/draftStore';

import { ComposerFields } from './ComposerFields';
import { Editor, insertAtSelection } from './Editor';
import { SnippetPicker } from './SnippetPicker';
import {
  fingerprintOf,
  mintCompositionId,
  opensInBody,
  registerComposerEditor,
  useComposerStore,
} from './composerStore';
import { editorToDocument } from './documentWire';
import { useAutosave } from './useAutosave';

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
export function SaveBadge({ composition }: { composition: CompositionId }) {
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
  const request = useComposerStore((s) => s.request);
  const sending = useComposerStore((s) => s.sending);
  const sendError = useComposerStore((s) => s.sendError);
  // A field's visibility is derived from its data: a toggle the user pressed
  // opens it, and a value the draft carries keeps it open, so a resumed
  // draft never hides the recipients it came with. The kind the composition
  // answers scopes what is offered at all: the threaded submits carry no
  // bcc, and the forward's no cc either, so those toggles never stand.
  const ccOffered = request === null || request.kind !== 'forward';
  const bccOffered = request === null;
  const [ccToggled, setCcToggled] = useState(false);
  const [bccToggled, setBccToggled] = useState(false);
  const ccShown = ccOffered && (ccToggled || cc !== '');
  const bccShown = bccOffered && (bccToggled || bcc !== '');
  // The live editor instance, handed over by the editor itself and registered
  // for the ribbon's inserts: the picker's insert places its snippet's body
  // at whatever selection now stands, and the opening caret lands in the
  // body through it.
  const editorRef = useRef<CoreEditor | null>(null);
  // The To field's input, handed over by the recipient field: the opening
  // caret lands in it on a new message.
  const toInputRef = useRef<HTMLInputElement | null>(null);

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

  // A composition inherits nothing from the one before it: the opening paths
  // reset the whole store, send state included, and the pane's own local
  // state moves with the composition id.

  // The opening caret goes where the work starts, decided by the one
  // predicate: in the body when the composer was already addressed, in To
  // otherwise. Placed once per composition, after the fields and the editor
  // have mounted with their seed, so moving the caret never reads as the
  // user having typed; an editor instance that arrives a frame late gets
  // its focus on that frame.
  useEffect(() => {
    if (!open || composition === null) {
      return;
    }
    const { request, to } = useComposerStore.getState();
    if (!opensInBody(request, to)) {
      toInputRef.current?.focus();
      return;
    }
    if (editorRef.current !== null) {
      editorRef.current.commands.focus();
      return;
    }
    const frame = requestAnimationFrame(() => {
      editorRef.current?.commands.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [composition, open]);

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

  const sendGated = to.trim() === '';

  return (
    <section
      aria-label="Composer"
      className="flex h-full w-full flex-col overflow-hidden bg-background"
    >
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between gap-2 border-b border-border bg-surface px-3">
        <SaveBadge composition={composition} />
        <div className="flex items-center gap-1">
          <SnippetPicker
            onInsert={(body) => {
              const editor = editorRef.current;
              if (editor !== null) {
                insertAtSelection(editor, body);
              }
            }}
          />
          <button
            type="button"
            onClick={() => useComposerStore.getState().submit()}
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
        key={`fields-${composition}`}
        fromLabel={fromLabel}
        to={to}
        cc={cc}
        bcc={bcc}
        subject={subject}
        ccOffered={ccOffered}
        ccShown={ccShown}
        bccOffered={bccOffered}
        bccShown={bccShown}
        toInputRef={toInputRef}
        onField={(field, value) => useComposerStore.getState().setField(field, value)}
        onToggleCc={() => setCcToggled((toggled) => !toggled)}
        onToggleBcc={() => setBccToggled((toggled) => !toggled)}
      />
      <Editor
        key={composition}
        initialContent={initialContent}
        onChange={(doc) => useComposerStore.getState().setEditorContent(doc.content ?? [])}
        onReady={(editor) => {
          editorRef.current = editor;
          registerComposerEditor(editor);
        }}
      />
    </section>
  );
}
