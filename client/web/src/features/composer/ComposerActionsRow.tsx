// SPDX-License-Identifier: GPL-3.0-only
// The composer actions row, kylins' ComposerActionsRow idiom narrowed to
// what the host backs: the primary Send on the left riding the store's one
// submit path, and on the right the snippet picker's insert seam, Discard,
// and Close when the pop-out window asks for it. No split-send caret and no
// encrypt, sign, or attach toggles: the composer has no backend for them.
// The row is shared by the pop-out window and the docked quick-reply strip.

import { dispatch, intents } from '@/services/ipc';
import { SentIcon } from '@/components/icons';

import { composerEditor, useComposerStore } from './composerStore';
import { editorToDocument } from './documentWire';
import { insertAtSelection } from './Editor';
import { SnippetPicker } from './SnippetPicker';

interface ComposerActionsRowProps {
  /** Whether the Close button stands: the pop-out window's, not the strip's. */
  showClose?: boolean;
}

/** Removes the composition's stored draft and closes it locally. */
function discard(): void {
  const composition = useComposerStore.getState().composition;
  if (composition === null) {
    return;
  }
  void dispatch(intents.discardDraft(composition));
  useComposerStore.getState().closePane();
}

/** Hands the whole current draft over as a final save, then closes locally. */
function close(): void {
  const state = useComposerStore.getState();
  if (state.composition === null) {
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
      then_close: true,
    }),
  );
  useComposerStore.getState().closePane();
}

export function ComposerActionsRow({ showClose = false }: ComposerActionsRowProps) {
  const to = useComposerStore((s) => s.to);
  const sending = useComposerStore((s) => s.sending);
  const sendGated = to.trim() === '';

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-1.5">
      <button
        type="button"
        onClick={() => useComposerStore.getState().submit()}
        disabled={sendGated || sending}
        title={sendGated ? 'Add a recipient first' : 'Send'}
        className="inline-flex items-center gap-1.5 rounded-[var(--radius-xs)] bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-50 min-h-[var(--hit)]"
      >
        <SentIcon size={13} />
        Send
      </button>
      <div className="flex-1" />
      <SnippetPicker
        onInsert={(body) => {
          const editor = composerEditor();
          if (editor !== null) {
            insertAtSelection(editor, body);
          }
        }}
      />
      <button
        type="button"
        onClick={discard}
        className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
      >
        Discard
      </button>
      {showClose && (
        <button
          type="button"
          aria-label="Close composer"
          onClick={close}
          className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
        >
          Close
        </button>
      )}
    </div>
  );
}
