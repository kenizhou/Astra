// SPDX-License-Identifier: GPL-3.0-only
// The reading pane: the right-hand column of the shell. Nothing open draws
// the empty state; a pulled snapshot draws its header, the invitation card
// when the message is a scheduling one, its AI replies surfaces (the smart
// replies strip, the summarise block, and the translate block, all keyed to
// the open message and standing absent with nothing to show), its body (the
// sandboxed HTML, the plain fallback, or a no-body note), and its
// attachment rows, with the close button crossing back as close_reading.
// The compose actions live on the command ribbon, their single home.

import { AttachmentList } from '@/components/reading-pane/AttachmentList';
import { InvitationCard } from '@/components/reading-pane/InvitationCard';
import { MessageHtml } from '@/components/reading-pane/MessageHtml';
import { ReadingHeader } from '@/components/reading-pane/ReadingHeader';
import { SmartReplies } from '@/components/reading-pane/SmartReplies';
import { SummaryBlock } from '@/components/reading-pane/SummaryBlock';
import { TranslateBlock } from '@/components/reading-pane/TranslateBlock';
import { CloseIcon, MailIcon } from '@/components/icons';
import { dispatch, intents } from '@/services/ipc';
import { useReadingStore } from '@/stores/readingStore';

export function ReadingPane() {
  const snapshot = useReadingStore((s) => s.snapshot);
  const showRemoteImages = useReadingStore((s) => s.showRemoteImages);
  const setShowRemoteImages = useReadingStore((s) => s.setShowRemoteImages);

  if (snapshot === null) {
    return (
      <aside
        aria-label="Reading pane"
        className="flex h-full w-full flex-col bg-background"
      >
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-4">
          <span className="flex h-12 w-12 items-center justify-center rounded-full bg-primary-muted text-primary">
            <MailIcon size={24} />
          </span>
          <p className="text-sm text-muted-text">Select a message to read it.</p>
        </div>
      </aside>
    );
  }

  return (
    <aside
      aria-label="Reading pane"
      className="flex h-full w-full flex-col overflow-hidden bg-background"
    >
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-end gap-2 border-b border-border-subtle bg-surface px-3">
        <button
          type="button"
          aria-label="Close reading pane"
          onClick={() => void dispatch(intents.closeReading())}
          className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <CloseIcon size={16} />
        </button>
      </div>
      <ReadingHeader
        subject={snapshot.subject}
        fromName={snapshot.from_name}
        fromAddress={snapshot.from_address}
        sentAt={snapshot.sent_at}
      />
      {snapshot.invitation !== undefined && (
        <InvitationCard message={snapshot.message} invitation={snapshot.invitation} />
      )}
      <SmartReplies />
      <SummaryBlock />
      <TranslateBlock />
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
