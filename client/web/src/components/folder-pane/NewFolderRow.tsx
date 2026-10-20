// SPDX-License-Identifier: GPL-3.0-only
// The folder tree's inline create row: an input standing where the new
// folder will sit, autofocused, Enter committing the create intent with
// the parent it hangs under, Escape and blur cancelling, and an empty
// name never committing. A refusal the host hands back shows its sentence
// and keeps the row open, never a silent no-op.

import { useEffect, useRef, useState } from 'react';

import { dispatch, intents } from '@/services/ipc';

interface NewFolderRowProps {
  /** The account the folder is created in. */
  account: string;
  /** The parent folder's key the create hangs under. */
  parent: string;
  /** The depth the row draws at: the parent's depth plus one. */
  depth: number;
  /** Closes the row, committed or cancelled. */
  onDone: () => void;
}

/** A refused create in words the row can show. */
function refusalMessage(err: unknown): string {
  if (err instanceof Error && err.message !== '') {
    return err.message;
  }
  return 'The folder could not be created';
}

export function NewFolderRow({ account, parent, depth, onDone }: NewFolderRowProps) {
  const [name, setName] = useState('');
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const commit = () => {
    const trimmed = name.trim();
    if (trimmed === '') {
      return;
    }
    setError(null);
    dispatch(intents.createFolder(account, trimmed, parent)).then(
      onDone,
      (err: unknown) => setError(refusalMessage(err)),
    );
  };

  return (
    <div
      role="treeitem"
      aria-level={depth + 1}
      className="relative flex min-h-[var(--row-h)] items-center gap-2 rounded-lg pr-2"
      style={{ paddingLeft: `${12 + depth * 14}px` }}
    >
      <span className="h-4 w-4 shrink-0" aria-hidden="true" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <input
          ref={inputRef}
          type="text"
          aria-label="New folder name"
          placeholder="Folder name"
          value={name}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              commit();
            } else if (event.key === 'Escape') {
              onDone();
            }
          }}
          onBlur={onDone}
          className="min-w-0 flex-1 rounded-full border border-input bg-background px-3 py-0.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
        />
        {error !== null && (
          <p role="alert" className="truncate pl-3 text-[11px] text-destructive">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}
