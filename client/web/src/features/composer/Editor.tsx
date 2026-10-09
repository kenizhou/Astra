// SPDX-License-Identifier: GPL-3.0-only
// The trimmed rich text editor: TipTap v3 at kylins' package versions, with
// StarterKit narrowed to the set the composer document carries (paragraphs,
// bold, italic, underline, both list kinds, links, hard breaks). Everything
// else the kit bundles is switched off, so the editor cannot produce a
// shape the wire would have to reject.

import { useRef } from 'react';
import { Editor as CoreEditor } from '@tiptap/core';
import { EditorContent, useEditor, type JSONContent } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';

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

interface EditorProps {
  /** The document's editable blocks, as TipTap doc content. */
  initialContent: JSONContent[];
  /** Hands the whole editor document over on every update. */
  onChange: (doc: JSONContent) => void;
}

export function Editor({ initialContent, onChange }: EditorProps) {
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

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

  return (
    <div className="min-h-0 flex-1 overflow-y-auto border-b border-border bg-background">
      <EditorContent
        editor={editor}
        className="px-4 py-3 text-[13px] leading-relaxed text-foreground"
      />
    </div>
  );
}
