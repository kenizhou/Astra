// SPDX-License-Identifier: GPL-3.0-only
// The attachment rows beneath the body: read-only metadata per part (the
// suggested file name, the media type, the decoded byte length rendered
// human). Saving and opening arrive with the attachments slice.

import type { AttachmentRow } from '@/services/ipc';

/** One size in bytes, rendered human: 512 B, 4.0 KB, 2.1 MB. */
function formatSize(size: number): string {
  if (size < 1024) {
    return `${size} B`;
  }
  const kb = size / 1024;
  if (kb < 1024) {
    return `${kb.toFixed(1)} KB`;
  }
  return `${(kb / 1024).toFixed(1)} MB`;
}

/** Draws the open message's attachments; an empty list draws nothing. */
export function AttachmentList({ attachments }: { attachments: AttachmentRow[] }) {
  if (attachments.length === 0) {
    return null;
  }
  return (
    <div aria-label="Attachments" className="shrink-0 border-t border-border-subtle">
      {attachments.map((attachment) => (
        <div
          key={attachment.id}
          className="flex items-baseline gap-2 border-b border-border px-4 py-2 text-xs last:border-b-0"
        >
          <span className="min-w-0 truncate text-foreground">{attachment.file_name}</span>
          <span className="shrink-0 text-muted-foreground">{attachment.media_type}</span>
          <span className="ml-auto shrink-0 text-muted-foreground">
            {formatSize(attachment.size)}
          </span>
        </div>
      ))}
    </div>
  );
}
