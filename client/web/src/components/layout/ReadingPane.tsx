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
import { ContextMenu, type ContextMenuItem } from '@/components/ui/ContextMenu';
import { useContextMenu } from '@/components/ui/useContextMenu';
import { EmptyState } from '@/components/ui/EmptyState';
import { STATE_CONTENT } from '@/components/ui/stateContent';
import { MailIcon } from '@/components/icons';
import { ComposerPane } from '@/features/composer/Composer';
import { useComposerStore } from '@/features/composer/composerStore';
import { usePeopleStore } from '@/features/people/peopleStore';
import { useReadingStore } from '@/stores/readingStore';
import { useUiStore } from '@/stores/uiStore';

/** The plain-text body's menu: Copy and Select All, no link items. */
function plainBodyItems(): ContextMenuItem[] {
  const selection = window.getSelection()?.toString() ?? '';
  return [
    {
      label: 'Copy',
      disabled: selection === '',
      title: selection === '' ? 'Select some text to copy' : undefined,
      description: selection === '' ? 'Select some text to copy' : undefined,
      onSelect: () => void navigator.clipboard?.writeText(selection),
    },
    {
      label: 'Select All',
      onSelect: () => {
        const pre = document.querySelector('[data-plain-body]');
        if (pre !== null) {
          window.getSelection()?.selectAllChildren(pre);
        }
      },
    },
  ];
}

export function ReadingPane() {
  const snapshot = useReadingStore((s) => s.snapshot);
  const showRemoteImages = useReadingStore((s) => s.showRemoteImages);
  const setShowRemoteImages = useReadingStore((s) => s.setShowRemoteImages);
  const inlineOpen = useComposerStore((s) => s.open && s.composition !== null);
  const plainMenu = useContextMenu();

  // kylins' InlineReply idiom: while the inline composer stands, it owns
  // the whole pane, the message chrome included; leaving the composition
  // (a send, a discard) hands the pane back to the message.
  if (inlineOpen) {
    return (
      <aside
        aria-label="Reading pane"
        className="flex h-full w-full flex-col overflow-hidden bg-[var(--paper)]"
      >
        <ComposerPane />
      </aside>
    );
  }

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
        message={snapshot.message}
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
        <pre
          data-plain-body
          onContextMenu={plainMenu.open}
          className="min-h-0 flex-1 overflow-y-auto whitespace-pre-wrap px-4 py-3 font-ui text-[13px] text-foreground"
        >
          {snapshot.plain}
        </pre>
      ) : (
        <p className="min-h-0 flex-1 p-4 text-sm text-muted-foreground">
          This message has no readable body.
        </p>
      )}
      <AttachmentList attachments={snapshot.attachments ?? []} />
      <QuickReplyStrip />
      {plainMenu.state !== null && (
        <ContextMenu
          x={plainMenu.state.x}
          y={plainMenu.state.y}
          items={plainBodyItems()}
          onClose={plainMenu.close}
        />
      )}
    </aside>
  );
}
