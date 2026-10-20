// SPDX-License-Identifier: GPL-3.0-only
// The folder tree's inline rename row: the NewFolderRow pattern over an
// existing folder, autofocused on the current name, Enter committing the
// rename intent, Escape and blur cancelling, and an unchanged or empty name
// never committing.

import { useEffect, useRef, useState } from 'react';

import { dispatch, intents } from '@/services/ipc';

interface RenameFolderRowProps {
  /** The account whose tree holds the folder. */
  account: string;
  /** The folder being renamed, by its key. */
  folder: string;
  /** The folder's current name, the input's opening value. */
  name: string;
  /** The depth the row draws at. */
  depth: number;
  /** Closes the row, committed or cancelled. */
  onDone: () => void;
}

export function RenameFolderRow({ account, folder, name, depth, onDone }: RenameFolderRowProps) {
  const [value, setName] = useState(name);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const commit = () => {
    const trimmed = value.trim();
    if (trimmed === '' || trimmed === name) {
      return;
    }
    void dispatch(intents.renameFolder(account, folder, trimmed));
    onDone();
  };

  return (
    <div
      role="treeitem"
      aria-level={depth + 1}
      className="relative flex min-h-[var(--row-h)] items-center gap-2 rounded-lg pr-2"
      style={{ paddingLeft: `${12 + depth * 14}px` }}
    >
      <span className="h-4 w-4 shrink-0" aria-hidden="true" />
      <input
        ref={inputRef}
        type="text"
        aria-label="Folder name"
        value={value}
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
    </div>
  );
}
