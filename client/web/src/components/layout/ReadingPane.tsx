// SPDX-License-Identifier: GPL-3.0-only
// The reading pane: the right-hand column of the shell. Nothing open draws
// the empty state; a pulled snapshot draws its header, its body (the
// sandboxed HTML, the plain fallback, or a no-body note), and its attachment
// rows, with the close button crossing back as close_reading.

import { AttachmentList } from '@/components/reading-pane/AttachmentList';
import { MessageHtml } from '@/components/reading-pane/MessageHtml';
import { ReadingHeader } from '@/components/reading-pane/ReadingHeader';
import { dispatch, intents } from '@/services/ipc';
import { useReadingStore } from '@/stores/readingStore';

/** A stroke-drawn cross glyph for the close button. */
function CloseGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="13"
      height="13"
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

export function ReadingPane() {
  const snapshot = useReadingStore((s) => s.snapshot);
  const showRemoteImages = useReadingStore((s) => s.showRemoteImages);
  const setShowRemoteImages = useReadingStore((s) => s.setShowRemoteImages);

  if (snapshot === null) {
    return (
      <aside
        aria-label="Reading pane"
        className="flex w-[440px] shrink-0 flex-col border-l border-border bg-background"
      >
        <p className="p-4 text-sm text-muted-foreground">Select a message to read it.</p>
      </aside>
    );
  }

  return (
    <aside
      aria-label="Reading pane"
      className="flex w-[440px] shrink-0 flex-col overflow-hidden border-l border-border bg-background"
    >
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-end border-b border-border bg-surface px-3">
        <button
          type="button"
          aria-label="Close reading pane"
          onClick={() => void dispatch(intents.closeReading())}
          className="rounded-[var(--radius-xs)] p-1 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <CloseGlyph />
        </button>
      </div>
      <ReadingHeader
        subject={snapshot.subject}
        fromName={snapshot.from_name}
        fromAddress={snapshot.from_address}
        sentAt={snapshot.sent_at}
      />
      {snapshot.html !== undefined ? (
        <MessageHtml
          html={snapshot.html}
          remoteImageCount={snapshot.remote_image_count}
          showRemoteImages={showRemoteImages}
          onShowRemoteImages={() => setShowRemoteImages(true)}
        />
      ) : snapshot.plain !== undefined ? (
        <pre className="min-h-0 flex-1 overflow-y-auto whitespace-pre-wrap px-4 py-3 font-ui text-[13px] text-foreground">
          {snapshot.plain}
        </pre>
      ) : (
        <p className="min-h-0 flex-1 p-4 text-sm text-muted-foreground">
          This message has no readable body.
        </p>
      )}
      <AttachmentList attachments={snapshot.attachments ?? []} />
    </aside>
  );
}
