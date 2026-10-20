// SPDX-License-Identifier: GPL-3.0-only
// The link popover, the in-place replacement for the window prompt: the
// target and the display text side by side, set and remove on the
// selection. The popover refuses nothing client-side except an empty
// target; the scheme allowlist stays core-side and the send-time sentence
// names what degrades.

import { useState } from 'react';
import { getMarkRange, type Editor as CoreEditor } from '@tiptap/core';

/** The popover's own view of one link: where it points and what it says. */
export interface LinkDraft {
  target: string;
  text: string;
}

/**
 * The link the selection stands on, as the popover's opening draft: the
 * active link's target, and the words of the selection (or of the link run
 * the caret sits in when the selection is collapsed).
 */
export function draftOfLink(editor: CoreEditor): LinkDraft {
  const { from, to } = editor.state.selection;
  let range = { from, to };
  const link = editor.schema.marks.link;
  if (from === to && link !== undefined) {
    const marked = getMarkRange(editor.state.doc.resolve(from), link);
    if (marked !== undefined) {
      range = marked;
    }
  }
  const href = editor.getAttributes('link').href;
  return {
    target: typeof href === 'string' ? href : '',
    text: editor.state.doc.textBetween(range.from, range.to, ' '),
  };
}

/**
 * Sets the draft on the selection: the words become `text` carrying the
 * target, or the standing selection keeps its words and takes the target.
 */
export function applyLink(editor: CoreEditor, draft: LinkDraft): void {
  const { from, to } = editor.state.selection;
  let range = { from, to };
  const link = editor.schema.marks.link;
  if (from === to && link !== undefined) {
    const marked = getMarkRange(editor.state.doc.resolve(from), link);
    if (marked !== undefined) {
      range = marked;
    }
  }
  if (draft.text.trim() !== '' && draft.text !== editor.state.doc.textBetween(from, to, ' ')) {
    editor
      .chain()
      .focus()
      .insertContentAt(range, {
        type: 'text',
        text: draft.text,
        marks: [{ type: 'link', attrs: { href: draft.target } }],
      })
      .run();
    return;
  }
  editor
    .chain()
    .focus()
    .setTextSelection(range)
    .setLink({ href: draft.target })
    .run();
}

interface LinkPopoverProps {
  /** The live editor the popover edits. */
  editor: CoreEditor;
  /** The draft the popover opens with. */
  initial: LinkDraft;
  /** Takes the popover down, whether or not anything was set. */
  onClose: () => void;
}

export function LinkPopover({ editor, initial, onClose }: LinkPopoverProps) {
  const [target, setTarget] = useState(initial.target);
  const [text, setText] = useState(initial.text);

  const set = (): void => {
    if (target.trim() === '') {
      return;
    }
    applyLink(editor, { target: target.trim(), text });
    onClose();
  };

  const remove = (): void => {
    editor.chain().focus().extendMarkRange('link').unsetLink().run();
    onClose();
  };

  const inputClass =
    'h-7 w-44 rounded-[var(--radius-xs)] border border-input bg-background px-2 text-xs text-foreground outline-none transition-colors focus:border-primary';

  return (
    <div
      role="dialog"
      aria-label="Link"
      className="absolute left-3 top-full z-[var(--z-popover)] mt-1 flex items-center gap-1.5 rounded-[var(--radius-sm)] border border-border bg-surface-floating px-2 py-1.5 shadow-[var(--shadow)]"
    >
      <input
        type="text"
        aria-label="Link target"
        value={target}
        autoFocus
        onChange={(event) => setTarget(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            set();
          }
          if (event.key === 'Escape') {
            onClose();
          }
        }}
        className={inputClass}
        placeholder="https://example.test"
      />
      <input
        type="text"
        aria-label="Display text"
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            set();
          }
          if (event.key === 'Escape') {
            onClose();
          }
        }}
        className={inputClass}
        placeholder="the words"
      />
      <button
        type="button"
        onClick={set}
        className="rounded-[var(--radius-xs)] bg-primary px-2 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
      >
        Set link
      </button>
      <button
        type="button"
        onClick={remove}
        className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
      >
        Remove link
      </button>
    </div>
  );
}
