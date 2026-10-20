// SPDX-License-Identifier: GPL-3.0-only
// The editor toolbar, kylins' EditorToolbar narrowed to what the trimmed
// StarterKit plus the widened marks loads (Editor.tsx's trimmedExtensions
// is the contract): bold, italic, underline, bullet and ordered lists, the
// font-size control over the shared catalog's four steps, two colour
// pickers from the app palette, the link popover, undo, and redo, and
// nothing else, because the editor can produce nothing else. Every toggle
// reflects the editor's active mark through useEditorState and acts through
// a focus-chained command; undo and redo stand disabled until history holds
// something. The link button and the Ctrl+K keybinding both open the
// popover, never a window prompt.

import { useEffect, useState, type ComponentType } from 'react';
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

import { EDITOR_PALETTE, FONT_STEPS } from './editorMarks';
import { LinkPopover, draftOfLink } from './LinkPopover';

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

/** One colour picker: the app palette as swatches, plus the remove control. */
function ColourPicker({
  groupLabel,
  removeLabel,
  mark,
  active,
  editor,
}: {
  groupLabel: string;
  removeLabel: string;
  mark: 'color' | 'highlight';
  active: string | null;
  editor: CoreEditor;
}) {
  return (
    <div role="group" aria-label={groupLabel} className="flex items-center gap-0.5">
      {EDITOR_PALETTE.map((entry) => (
        <button
          key={entry.hex}
          type="button"
          aria-label={`${groupLabel} ${entry.label}`}
          title={`${groupLabel} ${entry.label}`}
          aria-pressed={active === entry.hex}
          onClick={() => editor.chain().focus().setMark(mark, { color: entry.hex }).run()}
          className={`h-4 w-4 rounded-full border border-border transition-transform focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
            active === entry.hex ? 'ring-2 ring-primary' : 'hover:scale-110'
          }`}
          style={{ background: entry.hex }}
        />
      ))}
      <button
        type="button"
        aria-label={removeLabel}
        title={removeLabel}
        onClick={() => editor.chain().focus().unsetMark(mark).run()}
        className="ml-0.5 inline-flex h-7 w-7 items-center justify-center rounded-[var(--radius-xs)] text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <span aria-hidden="true" className="text-xs">
          {'\u00d7'}
        </span>
      </button>
    </div>
  );
}

interface EditorToolbarProps {
  /** The live composer editor; the bar draws nothing until one stands. */
  editor: CoreEditor | null;
}

export function EditorToolbar({ editor }: EditorToolbarProps) {
  const [linkOpen, setLinkOpen] = useState(false);

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
      fontSize: (current?.getAttributes('fontSize').size as string | undefined) ?? null,
      color: (current?.getAttributes('color').color as string | undefined) ?? null,
      highlight: (current?.getAttributes('highlight').color as string | undefined) ?? null,
      canUndo: current?.can().undo() ?? false,
      canRedo: current?.can().redo() ?? false,
    }),
  });

  // Ctrl+K opens the popover, the keybinding the button's title claims.
  useEffect(() => {
    if (editor === null) {
      return undefined;
    }
    const dom = editor.view.dom;
    const onKey = (event: KeyboardEvent): void => {
      if ((event.ctrlKey || event.metaKey) && (event.key === 'k' || event.key === 'K')) {
        event.preventDefault();
        setLinkOpen(true);
      }
    };
    dom.addEventListener('keydown', onKey);
    return () => dom.removeEventListener('keydown', onKey);
  }, [editor]);

  if (editor === null || state == null) {
    return null;
  }

  return (
    <div
      aria-label="Formatting"
      className="relative flex shrink-0 flex-nowrap items-center gap-0.5 overflow-x-auto border-b border-border bg-surface px-3 py-1"
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
      <select
        aria-label="Font size"
        title="Font size"
        value={state.fontSize ?? 'Normal'}
        onChange={(event) =>
          editor.chain().focus().setMark('fontSize', { size: event.target.value }).run()
        }
        className="h-7 rounded-[var(--radius-xs)] border border-border bg-background px-1 text-xs text-foreground outline-none transition-colors focus:border-primary"
      >
        {FONT_STEPS.map((step) => (
          <option key={step.id} value={step.id}>
            {step.label}
          </option>
        ))}
      </select>
      <ColourPicker
        groupLabel="Text colour"
        removeLabel="Remove text colour"
        mark="color"
        active={state.color}
        editor={editor}
      />
      <ColourPicker
        groupLabel="Highlight"
        removeLabel="Remove highlight"
        mark="highlight"
        active={state.highlight}
        editor={editor}
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
      <div className="relative">
        <ToolbarButton
          icon={LinkIcon}
          label="Insert link"
          title="Insert link (Ctrl+K)"
          pressed={state.link || linkOpen}
          onClick={() => setLinkOpen(true)}
        />
        {linkOpen && (
          <LinkPopover
            editor={editor}
            initial={draftOfLink(editor)}
            onClose={() => setLinkOpen(false)}
          />
        )}
      </div>
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
