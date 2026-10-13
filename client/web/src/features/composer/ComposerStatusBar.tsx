// SPDX-License-Identifier: GPL-3.0-only
// The composer status bar, kylins' ComposerStatusBar idiom narrowed to what
// the host offers: the 36px chrome footer with the draft save state on the
// left (the status surface's word for this composition alone) and the live
// word and character counts on the right, computed from the editor document
// and moving with every edit. No signature or template pickers: the composer
// has no backend for them.

import type { Editor as CoreEditor } from '@tiptap/core';
import { useEditorState } from '@tiptap/react';

import type { CompositionId } from '@/services/ipc';
import { useDraftStore } from '@/stores/draftStore';

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

/** The word count of one document text: whitespace-split, empties out. */
function countWords(text: string): number {
  return text.split(/\s+/).filter((word) => word !== '').length;
}

interface ComposerStatusBarProps {
  /** The live composer editor the counts follow; null counts nothing. */
  editor: CoreEditor | null;
  /** The composition the save badge matches against. */
  composition: CompositionId;
}

export function ComposerStatusBar({ editor, composition }: ComposerStatusBarProps) {
  const text = useEditorState({ editor, selector: ({ editor: current }) => current?.getText() ?? '' });
  const body = text ?? '';
  const words = countWords(body);
  const characters = body.length;

  return (
    <footer className="flex h-9 shrink-0 items-center justify-between border-t border-border bg-chrome px-3 text-xs text-muted-foreground">
      <SaveBadge composition={composition} />
      <span className="tabular-nums">
        {`${words} ${words === 1 ? 'word' : 'words'} · ${characters} ${characters === 1 ? 'character' : 'characters'}`}
      </span>
    </footer>
  );
}
