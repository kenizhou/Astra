// SPDX-License-Identifier: GPL-3.0-only
// The attachment rows beneath the body: read-only metadata per part (the
// suggested file name, the media type, the decoded byte length rendered
// human). Right-clicking a row opens its menu: Copy File Name and Save As
// work, Save As calling the attachment service and showing the outcome (the
// saved path or the refusal sentence), Download All looping it over every
// row, and previewing standing as a placeholder until its slice arrives.

import { useState } from 'react';

import type { AttachmentRow, MessageRef } from '@/services/ipc';
import { saveAttachment } from '@/services/attachments';
import { ContextMenu, placeholderItem, type ContextMenuItem } from '@/components/ui/ContextMenu';
import { useContextMenu } from '@/components/ui/useContextMenu';

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

interface AttachmentListProps {
  /** The open message's downloadable attachments. */
  attachments: AttachmentRow[];
  /** The open message: what a save names its parts by. */
  message: MessageRef;
}

/** The menu one attachment row opens. */
function buildAttachmentMenu(
  attachment: AttachmentRow,
  rows: AttachmentRow[],
  message: MessageRef,
  onOutcome: (note: string) => void,
): ContextMenuItem[] {
  const saveOne = (row: AttachmentRow): void => {
    void saveAttachment(message, row.id).then((outcome) => {
      if (outcome === 'cancelled') {
        return;
      }
      onOutcome('saved' in outcome ? `Saved to ${outcome.saved.path}` : outcome.failed.reason);
    });
  };
  return [
    placeholderItem('Preview', 'Attachment previews arrive with a future slice'),
    {
      label: 'Save As…',
      onSelect: () => saveOne(attachment),
    },
    ...(rows.length > 1
      ? [
          {
            label: 'Download All',
            onSelect: () => {
              // The saves run per row; the outcome line answers the last
              // row that reports.
              for (const row of rows) {
                saveOne(row);
              }
            },
          },
        ]
      : []),
    { separator: true },
    {
      label: 'Copy File Name',
      onSelect: () => void navigator.clipboard?.writeText(attachment.file_name),
    },
  ];
}

/** Draws the open message's attachments; an empty list draws nothing. */
export function AttachmentList({ attachments, message }: AttachmentListProps) {
  const menu = useContextMenu();
  const [target, setTarget] = useState<AttachmentRow | null>(null);
  const [note, setNote] = useState<string | null>(null);
  if (attachments.length === 0) {
    return null;
  }

  const openFor = (attachment: AttachmentRow, event: React.MouseEvent): void => {
    setTarget(attachment);
    menu.open(event);
  };

  return (
    <div aria-label="Attachments" className="shrink-0 border-t border-border-subtle">
      {note !== null && (
        <p role="status" className="border-b border-border-subtle px-4 py-1.5 text-xs text-muted-foreground">
          {note}
        </p>
      )}
      {attachments.map((attachment) => (
        <div
          key={attachment.id}
          tabIndex={0}
          onContextMenu={(event) => openFor(attachment, event)}
          onKeyDown={(event) => {
            if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
              event.preventDefault();
              const rect = event.currentTarget.getBoundingClientRect();
              openFor(attachment, {
                preventDefault: () => undefined,
                clientX: rect.left,
                clientY: rect.bottom,
              } as React.MouseEvent);
            }
          }}
          className="flex items-baseline gap-2 border-b border-border px-4 py-2 text-xs last:border-b-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
        >
          <span className="min-w-0 truncate text-foreground">{attachment.file_name}</span>
          <span className="shrink-0 text-muted-foreground">{attachment.media_type}</span>
          <span className="ml-auto shrink-0 text-muted-foreground">
            {formatSize(attachment.size)}
          </span>
        </div>
      ))}
      {menu.state !== null && target !== null && (
        <ContextMenu
          x={menu.state.x}
          y={menu.state.y}
          items={buildAttachmentMenu(target, attachments, message, setNote)}
          onClose={menu.close}
        />
      )}
    </div>
  );
}
