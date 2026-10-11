// SPDX-License-Identifier: GPL-3.0-only
// The trimmed rich text editor: TipTap v3 at kylins' package versions, with
// StarterKit narrowed to the set the composer document carries (paragraphs,
// bold, italic, underline, both list kinds, links, hard breaks). Everything
// else the kit bundles is switched off, so the editor cannot produce a
// shape the wire would have to reject. The editor also hands its live
// instance to the pane (the snippet picker's insert seam), carries the
// autocomplete chip anchored to its own scroll box, and exports the
// place-plain-text-at-the-selection discipline both inserts share.

import { useEffect, useRef } from 'react';
import { Editor as CoreEditor } from '@tiptap/core';
import { EditorContent, useEditor, type JSONContent } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';

import { AutocompleteChip } from './AutocompleteChip';

/**
 * The trimmed extension set: one array shared by the mounted editor and the
 * bare editors the wire tests cross through, so the schema they accept can
 * never drift apart.
 */
export function trimmedExtensions() {
  return [
    StarterKit.configure({
      heading: false,
      blockquote: false,
      code: false,
      codeBlock: false,
      horizontalRule: false,
      strike: false,
      trailingNode: false,
      link: { openOnClick: false },
    }),
  ];
}

/** A bare editor over the trimmed set, for callers outside React. */
export function createComposerEditor(content: JSONContent[]): CoreEditor {
  return new CoreEditor({
    element: document.createElement('div'),
    extensions: trimmedExtensions(),
    content: { type: 'doc', content },
  });
}

/**
 * Plain text as editor content: a single line stays one inline run inside
 * the current paragraph, and each further line becomes its own paragraph,
 * so a body's newlines survive the insert instead of collapsing as parsed
 * HTML.
 */
function plainTextContent(text: string): JSONContent | JSONContent[] {
  if (!text.includes('\n')) {
    return { type: 'text', text };
  }
  return text.split('\n').map((line) => ({
    type: 'paragraph',
    content: line === '' ? [] : [{ type: 'text', text: line }],
  }));
}

/** Places plain text at the editor's current selection, replacing it. */
export function insertAtSelection(editor: CoreEditor, text: string): void {
  editor.chain().focus().insertContentAt(editor.state.selection, plainTextContent(text)).run();
}

interface EditorProps {
  /** The document's editable blocks, as TipTap doc content. */
  initialContent: JSONContent[];
  /** Hands the whole editor document over on every update. */
  onChange: (doc: JSONContent) => void;
  /** Handed the live editor once created, and null again on unmount. */
  onReady?: (editor: CoreEditor | null) => void;
}

export function Editor({ initialContent, onChange, onReady }: EditorProps) {
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;
  const wrapRef = useRef<HTMLDivElement>(null);

  const editor = useEditor({
    extensions: trimmedExtensions(),
    content: { type: 'doc', content: initialContent },
    onUpdate: ({ editor: updated }) => {
      onChangeRef.current(updated.getJSON());
    },
    editorProps: {
      attributes: {
        'aria-label': 'Message body',
        class: 'min-h-full outline-none',
      },
    },
  });

  useEffect(() => {
    const ready = onReadyRef.current;
    ready?.(editor ?? null);
    return () => ready?.(null);
  }, [editor]);

  return (
    <div
      ref={wrapRef}
      className="relative min-h-0 flex-1 overflow-y-auto border-b border-border bg-background"
    >
      <EditorContent
        editor={editor}
        className="px-4 py-3 text-[13px] leading-relaxed text-foreground"
      />
      {editor !== null && (
        <AutocompleteChip
          editor={editor}
          container={wrapRef}
          onInsert={(text) => insertAtSelection(editor, text)}
        />
      )}
    </div>
  );
}
