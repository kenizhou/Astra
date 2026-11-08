// SPDX-License-Identifier: GPL-3.0-only
// The whole reading view both readers render: the pane's card and the
// pop-out window's root. It draws the header (whose verbs carry the junk
// context), the invitation card when the message is a scheduling one, the
// AI surfaces keyed to the open message, the body (the sandboxed HTML at
// the standing zoom, the plain fallback on the same steps, or a no-body
// note), and the attachment rows, with the quick-reply strip docked at the
// bottom. The junk judgement is the browsing context: the folder whose
// list the message was opened from, resolved against the account's tree,
// gated by the provider's report capability.

import { useContextMenu } from '@/components/ui/useContextMenu';
import { ContextMenu, type ContextMenuItem } from '@/components/ui/ContextMenu';
import { AttachmentList } from '@/components/reading-pane/AttachmentList';
import { InvitationCard } from '@/components/reading-pane/InvitationCard';
import { MessageHtml, READING_ZOOM_BASE_PX } from '@/components/reading-pane/MessageHtml';
import { QuickReplyStrip } from '@/components/reading-pane/QuickReplyStrip';
import { ReadingHeader } from '@/components/reading-pane/ReadingHeader';
import { SmartReplies } from '@/components/reading-pane/SmartReplies';
import { SummaryBlock } from '@/components/reading-pane/SummaryBlock';
import { TranslateBlock } from '@/components/reading-pane/TranslateBlock';
import { PluginBanner } from '@/features/plugins/PluginBanner';
import { useAppearanceStore } from '@/services/appearance';
import { useFolderStore } from '@/stores/folderStore';
import { useReadingStore } from '@/stores/readingStore';

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

export interface ReadingViewProps {
  /** When set and a sender stands, the view offers an add-contact chip. */
  onAddContact?: () => void;
  /** The folder whose list the message was opened from, for the Not-spam
   *  verb's junk-folder judgement; null when none is known. */
  openedFromFolder?: { account: string; key: string } | null;
}

export function ReadingView({ onAddContact, openedFromFolder = null }: ReadingViewProps) {
  const snapshot = useReadingStore((s) => s.snapshot);
  const showRemoteImages = useReadingStore((s) => s.showRemoteImages);
  const setShowRemoteImages = useReadingStore((s) => s.setShowRemoteImages);
  const zoom = useAppearanceStore((s) => s.readingZoom);
  const accounts = useFolderStore((s) => s.accounts);
  const plainMenu = useContextMenu();

  if (snapshot === null) {
    return null;
  }

  // The junk judgement: the folder the message was opened from is the
  // account's junk folder (by the tree's role), and the account's provider
  // can report. The folder key is resolved against the message's own
  // account, so a folder of another account names nothing.
  const group =
    openedFromFolder !== null
      ? accounts.find((row) => row.account_id === snapshot.message.account)
      : undefined;
  const openedFromJunk =
    group !== undefined &&
    group.can_report_mail === true &&
    openedFromFolder !== null &&
    group.folders.some(
      (folder) => folder.key === openedFromFolder.key && folder.role === 'junk',
    );

  return (
    <>
      <ReadingHeader
        subject={snapshot.subject}
        fromName={snapshot.from_name}
        fromAddress={snapshot.from_address}
        sentAt={snapshot.sent_at}
        message={snapshot.message}
        onAddContact={onAddContact}
        openedFromFolder={openedFromFolder}
        notJunk={openedFromJunk}
      />
      {snapshot.invitation !== undefined && (
        <InvitationCard message={snapshot.message} invitation={snapshot.invitation} />
      )}
      {snapshot.security_banner !== undefined && (
        <PluginBanner banner={snapshot.security_banner} />
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
          zoom={zoom}
        />
      ) : snapshot.plain !== undefined ? (
        <pre
          data-plain-body
          onContextMenu={plainMenu.open}
          style={{ fontSize: `${READING_ZOOM_BASE_PX[zoom]}px` }}
          className="min-h-0 flex-1 overflow-y-auto whitespace-pre-wrap px-4 py-3 font-ui text-foreground"
        >
          {snapshot.plain}
        </pre>
      ) : (
        <p className="min-h-0 flex-1 p-4 text-sm text-muted-foreground">
          This message has no readable body.
        </p>
      )}
      <AttachmentList attachments={snapshot.attachments ?? []} message={snapshot.message} />
      <QuickReplyStrip />
      {plainMenu.state !== null && (
        <ContextMenu
          x={plainMenu.state.x}
          y={plainMenu.state.y}
          items={plainBodyItems()}
          onClose={plainMenu.close}
        />
      )}
    </>
  );
}
