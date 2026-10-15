// SPDX-License-Identifier: GPL-3.0-only
// The reading pane: the right-hand column of the shell. Nothing open draws
// the empty state; a pulled snapshot draws its header, the invitation card
// when the message is a scheduling one, its AI replies surfaces (the smart
// replies strip, the summarise block, and the translate block, all keyed to
// the open message and standing absent with nothing to show), its body (the
// sandboxed HTML, the plain fallback, or a no-body note), and its
// attachment rows, with the close button crossing back as close_reading.
// The quick-reply strip docks at the bottom: a one-line Reply affordance
// that expands into the inline composer. The pop-out compose actions live
// on the command ribbon, their single home.

import { AttachmentList } from '@/components/reading-pane/AttachmentList';
import { InvitationCard } from '@/components/reading-pane/InvitationCard';
import { MessageHtml } from '@/components/reading-pane/MessageHtml';
import { QuickReplyStrip } from '@/components/reading-pane/QuickReplyStrip';
import { ReadingHeader } from '@/components/reading-pane/ReadingHeader';
import { SmartReplies } from '@/components/reading-pane/SmartReplies';
import { SummaryBlock } from '@/components/reading-pane/SummaryBlock';
import { TranslateBlock } from '@/components/reading-pane/TranslateBlock';
import { EmptyState } from '@/components/ui/EmptyState';
import { STATE_CONTENT } from '@/components/ui/stateContent';
import { MailIcon } from '@/components/icons';
import { dispatch, intents } from '@/services/ipc';
import { usePeopleStore } from '@/features/people/peopleStore';
import { useReadingStore } from '@/stores/readingStore';
import { useUiStore } from '@/stores/uiStore';

export function ReadingPane() {
  const snapshot = useReadingStore((s) => s.snapshot);
  const showRemoteImages = useReadingStore((s) => s.showRemoteImages);
  const setShowRemoteImages = useReadingStore((s) => s.setShowRemoteImages);

  if (snapshot === null) {
    return (
      <aside
        aria-label="Reading pane"
        className="flex h-full w-full flex-col bg-[var(--paper)]"
      >
        <EmptyState glyph={<MailIcon size={24} />} message={STATE_CONTENT.readingEmpty!.message} />
        {/* The strip's expanded state stands even with nothing open: the app
            start resume opens its draft here. */}
        <QuickReplyStrip />
      </aside>
    );
  }

  return (
    <aside
      aria-label="Reading pane"
      className="flex h-full w-full flex-col overflow-hidden bg-[var(--paper)]"
    >
      <ReadingHeader
        subject={snapshot.subject}
        fromName={snapshot.from_name}
        fromAddress={snapshot.from_address}
        sentAt={snapshot.sent_at}
        onClose={() => void dispatch(intents.closeReading())}
        onAddContact={() => {
          usePeopleStore.getState().requestContactCreate({
            name: snapshot.from_name,
            address: snapshot.from_address,
          });
          useUiStore.getState().setShellView('people');
        }}
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
      <QuickReplyStrip />
    </aside>
  );
}
