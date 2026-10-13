// SPDX-License-Identifier: GPL-3.0-only
// The editor toolbar, kylins' EditorToolbar narrowed to what the trimmed
// StarterKit loads (Editor.tsx's trimmedExtensions is the contract): bold,
// italic, underline, bullet and ordered lists, link, undo, and redo, and
// nothing else, because the editor can produce nothing else. Every toggle
// reflects the editor's active mark through useEditorState and acts through
// a focus-chained command; undo and redo stand disabled until history holds
// something. The link button sets the prompted target on the selection, or
// unsets the link the caret sits on.

import type { ComponentType } from 'react';
import type { Editor as CoreEditor } from '@tiptap/core';
import { useEditorState } from '@tiptap/react';

import {
  BoldIcon,
  BulletListIcon,
  ItalicIcon,
  LinkIcon,
  OrderedListIcon,
  RedoIcon,
  UnderlineIcon,
  UndoIcon,
  type IconProps,
} from '@/components/icons';

interface ToolbarButtonProps {
  icon: ComponentType<IconProps>;
  label: string;
  title: string;
  pressed?: boolean;
  disabled?: boolean;
  onClick: () => void;
}

function ToolbarButton({ icon: Icon, label, title, pressed, disabled, onClick }: ToolbarButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={pressed}
      disabled={disabled}
      title={title}
      onClick={onClick}
      className={`inline-flex h-7 w-7 items-center justify-center rounded-[var(--radius-xs)] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40 ${
        pressed === true ? 'bg-primary-muted text-primary' : 'text-muted-foreground hover:bg-hover hover:text-foreground'
      }`}
    >
      <Icon size={15} />
    </button>
  );
}

function ToolbarDivider() {
  return <div aria-hidden="true" className="mx-1 h-4 w-px bg-border" />;
}

interface EditorToolbarProps {
  /** The live composer editor; the bar draws nothing until one stands. */
  editor: CoreEditor | null;
}

export function EditorToolbar({ editor }: EditorToolbarProps) {
  // One granular subscription: the bar re-renders only when one of these
  // flags flips, and the selector stays null-safe before the editor stands.
  const state = useEditorState({
    editor,
    selector: ({ editor: current }) => ({
      bold: current?.isActive('bold') ?? false,
      italic: current?.isActive('italic') ?? false,
      underline: current?.isActive('underline') ?? false,
      bulletList: current?.isActive('bulletList') ?? false,
      orderedList: current?.isActive('orderedList') ?? false,
      link: current?.isActive('link') ?? false,
      canUndo: current?.can().undo() ?? false,
      canRedo: current?.can().redo() ?? false,
    }),
  });

  if (editor === null || state == null) {
    return null;
  }

  const editLink = (): void => {
    if (editor.isActive('link')) {
      editor.chain().focus().unsetLink().run();
      return;
    }
    const href = window.prompt('Link target');
    if (href !== null && href.trim() !== '') {
      editor.chain().focus().setLink({ href: href.trim() }).run();
    }
  };

  return (
    <div
      aria-label="Formatting"
      className="flex shrink-0 flex-nowrap items-center gap-0.5 overflow-x-auto border-b border-border bg-surface px-3 py-1"
    >
      <ToolbarButton
        icon={BoldIcon}
        label="Bold"
        title="Bold (Ctrl+B)"
        pressed={state.bold}
        onClick={() => editor.chain().focus().toggleBold().run()}
      />
      <ToolbarButton
        icon={ItalicIcon}
        label="Italic"
        title="Italic (Ctrl+I)"
        pressed={state.italic}
        onClick={() => editor.chain().focus().toggleItalic().run()}
      />
      <ToolbarButton
        icon={UnderlineIcon}
        label="Underline"
        title="Underline (Ctrl+U)"
        pressed={state.underline}
        onClick={() => editor.chain().focus().toggleUnderline().run()}
      />
      <ToolbarDivider />
      <ToolbarButton
        icon={BulletListIcon}
        label="Bullet list"
        title="Bullet list"
        pressed={state.bulletList}
        onClick={() => editor.chain().focus().toggleBulletList().run()}
      />
      <ToolbarButton
        icon={OrderedListIcon}
        label="Numbered list"
        title="Numbered list"
        pressed={state.orderedList}
        onClick={() => editor.chain().focus().toggleOrderedList().run()}
      />
      <ToolbarDivider />
      <ToolbarButton
        icon={LinkIcon}
        label="Insert link"
        title="Insert link (Ctrl+K)"
        pressed={state.link}
        onClick={editLink}
      />
      <div className="flex-1" />
      <ToolbarButton
        icon={UndoIcon}
        label="Undo"
        title="Undo (Ctrl+Z)"
        disabled={!state.canUndo}
        onClick={() => editor.chain().focus().undo().run()}
      />
      <ToolbarButton
        icon={RedoIcon}
        label="Redo"
        title="Redo (Ctrl+Y)"
        disabled={!state.canRedo}
        onClick={() => editor.chain().focus().redo().run()}
      />
    </div>
  );
}
