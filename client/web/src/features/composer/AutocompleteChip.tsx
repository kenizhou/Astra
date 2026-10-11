// SPDX-License-Identifier: GPL-3.0-only
// The compose autocomplete chip: two seconds after typing stops the chip
// asks the host for the continuation at the cursor (the text before it,
// plus the text after it only when the cursor sits inside text), skips
// the ask over an unchanged context or an empty editor, and shows a non
// empty answer as a dimmed suggestion anchored under the current line.
// The suggestion never inserts itself: only a Tab or a click accepts it,
// and a dismiss hides the chip for the rest of the session. A run that
// settles after typing resumed is dropped, the summarise block's epoch
// discipline, so a stale continuation can never land under new words.

import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';
import type { Editor as CoreEditor } from '@tiptap/core';

import { composeAutocomplete } from '@/services/ai';

/** How long the editor must stay quiet before the ask fires. */
const IDLE_MS = 2000;

interface AutocompleteChipProps {
  /** The live composer editor whose updates arm the idle ask. */
  editor: CoreEditor;
  /** The chip's anchoring container, the editor's own scroll box. */
  container: RefObject<HTMLElement | null>;
  /** Inserts one accepted suggestion at the cursor, the gesture's seam. */
  onInsert: (text: string) => void;
}

/** The editor's plain text around the cursor, the command's two halves. */
function contextOf(editor: CoreEditor): { before: string; after: string } {
  const { from, to } = editor.state.selection;
  const doc = editor.state.doc;
  return {
    before: doc.textBetween(0, from, '\n'),
    after: doc.textBetween(to, doc.content.size, '\n'),
  };
}

/** The caret's box offset against the container, or null when unknowable. */
function chipOffset(
  editor: CoreEditor,
  container: RefObject<HTMLElement | null>,
): { top: number; left: number } | null {
  try {
    const caret = editor.view.coordsAtPos(editor.state.selection.from);
    const box = container.current?.getBoundingClientRect();
    if (box === undefined) {
      return null;
    }
    return { top: caret.bottom - box.top, left: Math.max(caret.left - box.left, 0) };
  } catch {
    // A caret the view cannot place still shows the chip, pinned to the
    // container's own corner.
    return null;
  }
}

/** A stroke-drawn cross glyph for the dismiss control. */
function DismissGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="9"
      height="9"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </svg>
  );
}

export function AutocompleteChip({ editor, container, onInsert }: AutocompleteChipProps) {
  const [suggestion, setSuggestion] = useState<string | null>(null);
  // The idle window's timer, the context already asked for, the session's
  // dismissal, and the epoch a settling answer must still hold.
  const timer = useRef<number | null>(null);
  const askedFor = useRef<string | null>(null);
  const dismissed = useRef(false);
  const runEpoch = useRef(0);
  const onInsertRef = useRef(onInsert);
  onInsertRef.current = onInsert;

  useEffect(() => {
    const arm = (afterType: () => void): void => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
      }
      timer.current = window.setTimeout(afterType, IDLE_MS);
    };

    const ask = (): void => {
      const { before, after } = contextOf(editor);
      const context = `${before}\u0000${after}`;
      if (before.trim() === '' || askedFor.current === context) {
        return;
      }
      askedFor.current = context;
      const run = ++runEpoch.current;
      const call =
        after === '' ? composeAutocomplete(before) : composeAutocomplete(before, after);
      call.then(
        (text) => {
          if (runEpoch.current !== run) {
            return;
          }
          setSuggestion(text === '' ? null : text);
        },
        (err: unknown) => {
          console.error('compose: the autocomplete ask failed', err);
        },
      );
    };

    const onType = (): void => {
      // Typing makes any standing suggestion stale and any in-flight ask
      // spent, then restarts the idle window from this keystroke.
      setSuggestion(null);
      runEpoch.current += 1;
      if (dismissed.current) {
        return;
      }
      arm(ask);
    };

    editor.on('update', onType);
    return () => {
      editor.off('update', onType);
      runEpoch.current += 1;
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
        timer.current = null;
      }
    };
  }, [editor]);

  const accept = useCallback((): void => {
    if (suggestion === null) {
      return;
    }
    setSuggestion(null);
    onInsertRef.current(suggestion);
  }, [suggestion]);

  const dismiss = useCallback((): void => {
    dismissed.current = true;
    runEpoch.current += 1;
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
    setSuggestion(null);
  }, []);

  // Tab accepts while a suggestion stands, wherever the focus sits.
  useEffect(() => {
    if (suggestion === null) {
      return;
    }
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Tab') {
        event.preventDefault();
        accept();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [suggestion, accept]);

  if (suggestion === null) {
    return null;
  }
  const offset = chipOffset(editor, container);
  return (
    <div
      style={
        offset === null
          ? { bottom: '4px', left: '4px' }
          : { top: `${offset.top}px`, left: `${offset.left}px` }
      }
      className="absolute z-10 flex items-center gap-0.5 rounded-[var(--radius-xs)] border border-border bg-surface px-1 py-0.5 shadow-md"
    >
      <button
        type="button"
        onClick={accept}
        title="Tab to insert"
        className="max-w-[280px] truncate text-left text-xs italic text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {suggestion}
      </button>
      <button
        type="button"
        aria-label="Dismiss suggestion"
        onClick={dismiss}
        className="rounded-[var(--radius-xs)] p-0.5 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <DismissGlyph />
      </button>
    </div>
  );
}
