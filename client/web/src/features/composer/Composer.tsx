// SPDX-License-Identifier: GPL-3.0-only
// The composer surface and its session: a landed compose request opens a
// pop-out window per composition (the routing lives in useComposeRequest.ts
// and composeWindow.ts), the app start resume opens the stored draft into
// the docked quick-reply strip, and this one pane serves both surfaces, the
// window passing showClose for its Close button. The actions row carries
// Send, Snippets, Discard, and that Close; the editor toolbar and the status
// bar sit above and under the trimmed editor; the idle autosave hands the
// whole draft to the core. A composition a request opened carries its kind
// and threading header until the pane leaves, so its saves keep the quote
// and its send rides the threaded submit. Every close dismisses the request
// the composition came from. The live editor registers with the store, so
// the actions row's and the ribbon's inserts land at the current selection.

import { useEffect, useRef, useState } from 'react';
import type { Editor as CoreEditor } from '@tiptap/core';

import { draftResume } from '@/services/ipc';
import { isTauri } from '@/events/surfaceEvents';
import { dispatch, intents } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';

import { ComposerActionsRow } from './ComposerActionsRow';
import { ComposerFields } from './ComposerFields';
import { ComposerStatusBar } from './ComposerStatusBar';
import { Editor } from './Editor';
import { EditorToolbar } from './EditorToolbar';
import {
  fingerprintOf,
  mintCompositionId,
  opensInBody,
  registerComposerEditor,
  useComposerStore,
} from './composerStore';
import { editorToDocument } from './documentWire';
import { useAutosave } from './useAutosave';

// The save badge lives with the status bar now; the ribbon still imports it
// from here.
export { SaveBadge } from './ComposerStatusBar';

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

interface ComposerPaneProps {
  /** Whether the actions row carries Close: the pop-out window's, not the strip's. */
  showClose?: boolean;
}

export function ComposerPane({ showClose = false }: ComposerPaneProps) {
  const open = useComposerStore((s) => s.open);
  const composition = useComposerStore((s) => s.composition);
  const fromLabel = useComposerStore((s) => s.fromLabel);
  const to = useComposerStore((s) => s.to);
  const cc = useComposerStore((s) => s.cc);
  const bcc = useComposerStore((s) => s.bcc);
  const subject = useComposerStore((s) => s.subject);
  const initialContent = useComposerStore((s) => s.editor.content);
  const request = useComposerStore((s) => s.request);
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
  // The live editor instance, in state so the toolbar and the status bar
  // follow it: the toolbar reflects its active marks, the status bar counts
  // its document. The store registration beside it serves the inserts from
  // outside the pane.
  const [editor, setEditor] = useState<CoreEditor | null>(null);
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
  // otherwise. Placed when the fields and the editor stand, so moving the
  // caret never reads as the user having typed.
  useEffect(() => {
    if (!open || composition === null) {
      return;
    }
    const { request, to } = useComposerStore.getState();
    if (!opensInBody(request, to)) {
      toInputRef.current?.focus();
      return;
    }
    editor?.commands.focus();
  }, [composition, open, editor]);

  if (!open || composition === null) {
    return null;
  }

  return (
    <section
      aria-label="Composer"
      className="flex h-full w-full flex-col overflow-hidden bg-background"
    >
      <ComposerActionsRow showClose={showClose} />
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
      <EditorToolbar editor={editor} />
      <Editor
        key={composition}
        initialContent={initialContent}
        onChange={(doc) => useComposerStore.getState().setEditorContent(doc.content ?? [])}
        onReady={(instance) => {
          registerComposerEditor(instance);
          setEditor(instance);
        }}
      />
      <ComposerStatusBar editor={editor} composition={composition} />
    </section>
  );
}
