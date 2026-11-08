// SPDX-License-Identifier: GPL-3.0-only
// The composer's attachment surface: the Attach button over the webview's
// own multi-select input (the bytes never round-trip through the host), the
// document's attachment rows with their remove controls, the inline-or-
// file ask a dropped picture gets, and the OS share hand-off's pending
// rows, named by the file and marked pending (their bytes stage when the
// read seam ships; the path never renders). The rows render whatever the
// document holds, so a forward's pre-attached originals stand here too.

import { useRef } from 'react';

import { CloseIcon, PaperclipIcon } from '@/components/icons';

import { useComposerStore } from './composerStore';

/** One size in bytes, rendered human: 512 B, 4.0 KB, 2.1 MB. */
function formatSize(size: number | null): string {
  if (size === null) {
    return '';
  }
  if (size < 1024) {
    return `${size} B`;
  }
  const kb = size / 1024;
  if (kb < 1024) {
    return `${kb.toFixed(1)} KB`;
  }
  return `${(kb / 1024).toFixed(1)} MB`;
}

export function ComposerAttachments() {
  const attachments = useComposerStore((s) => s.editor.attachments);
  const pendingImages = useComposerStore((s) => s.pendingImages);
  const pendingShares = useComposerStore((s) => s.pendingShares);
  const inputRef = useRef<HTMLInputElement>(null);

  const onPick = (event: React.ChangeEvent<HTMLInputElement>): void => {
    useComposerStore.getState().attachFiles(Array.from(event.target.files ?? []));
    // The same file picked twice must land twice: the input's value clears.
    event.target.value = '';
  };

  return (
    <div
      aria-label="Attachments"
      className="shrink-0 border-t border-border-subtle bg-surface px-3 py-1.5"
    >
      <div className="flex items-center gap-1.5">
        <button
          type="button"
          aria-label="Attach files"
          onClick={() => inputRef.current?.click()}
          className="inline-flex items-center gap-1 rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
        >
          <PaperclipIcon size={13} />
          Attach
        </button>
        <input
          ref={inputRef}
          type="file"
          multiple
          aria-label="Attach files"
          className="hidden"
          onChange={onPick}
        />
      </div>
      {attachments.map((row) => (
        <div
          key={row.id}
          className="flex items-baseline gap-2 border-t border-border-subtle py-1 text-xs first-of-type:mt-1.5"
        >
          <span className="min-w-0 truncate text-foreground">{row.file_name}</span>
          {typeof row.disposition !== 'string' && (
            <span className="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[10px] leading-none text-muted-text">
              inline
            </span>
          )}
          <span className="shrink-0 text-muted-foreground">{row.media_type}</span>
          <span className="ml-auto shrink-0 text-muted-foreground">{formatSize(row.size)}</span>
          <button
            type="button"
            aria-label={`Remove ${row.file_name}`}
            onClick={() => useComposerStore.getState().removeAttachment(row.id)}
            className="flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <CloseIcon size={12} />
          </button>
        </div>
      ))}
      {pendingShares.map((file) => (
        <div
          key={`pending-${file.name}:${file.path}`}
          className="flex items-baseline gap-2 border-t border-border-subtle py-1 text-xs"
        >
          <span className="min-w-0 truncate text-foreground">{file.name}</span>
          <span className="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[10px] leading-none text-muted-text">
            pending
          </span>
          <button
            type="button"
            aria-label={`Remove pending ${file.name}`}
            onClick={() => useComposerStore.getState().removeShare(file)}
            className="ml-auto flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <CloseIcon size={12} />
          </button>
        </div>
      ))}
      {pendingImages.length > 0 && (
        <div
          role="dialog"
          aria-label="Picture disposition"
          className="mt-1.5 flex items-center gap-2 rounded-[var(--radius-sm)] border border-border-subtle bg-background px-2 py-1.5 text-xs"
        >
          <span className="min-w-0 flex-1 truncate text-foreground">
            {pendingImages.length === 1
              ? `How should ${pendingImages[0]!.name} ride this message?`
              : `How should the ${pendingImages.length} pictures ride this message?`}
          </span>
          <button
            type="button"
            onClick={() => useComposerStore.getState().answerPendingImages(true)}
            className="whitespace-nowrap rounded-[var(--radius-xs)] bg-primary px-2 py-0.5 font-medium text-primary-foreground transition-colors hover:bg-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Insert inline
          </button>
          <button
            type="button"
            onClick={() => useComposerStore.getState().answerPendingImages(false)}
            className="whitespace-nowrap rounded-[var(--radius-xs)] px-2 py-0.5 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Attach as file
          </button>
        </div>
      )}
    </div>
  );
}
