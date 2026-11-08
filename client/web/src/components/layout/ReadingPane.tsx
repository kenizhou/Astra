// SPDX-License-Identifier: GPL-3.0-only
// The reading pane: the right-hand column of the shell. Nothing open draws
// the empty state; a pulled snapshot draws the whole reading view over the
// shared component the pop-out window renders too, with the folder the
// list sits in riding along so the Not-spam verb's junk judgement holds.
// The quick-reply strip docks at the bottom: a one-line Reply affordance
// that expands into the inline composer. The pop-out compose actions live
// on the command ribbon, their single home.

import { EmptyState } from '@/components/ui/EmptyState';
import { QuickReplyStrip } from '@/components/reading-pane/QuickReplyStrip';
import { ReadingView } from '@/components/reading-pane/ReadingView';
import { STATE_CONTENT } from '@/components/ui/stateContent';
import { MailIcon } from '@/components/icons';
import { ComposerPane } from '@/features/composer/Composer';
import { useComposerStore } from '@/features/composer/composerStore';
import { usePeopleStore } from '@/features/people/peopleStore';
import { useReadingStore } from '@/stores/readingStore';
import { useFolderStore } from '@/stores/folderStore';
import { useUiStore } from '@/stores/uiStore';

export function ReadingPane() {
  const snapshot = useReadingStore((s) => s.snapshot);
  const inlineOpen = useComposerStore((s) => s.open && s.composition !== null);
  const selectedFolder = useFolderStore((s) => s.selected);

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
      <ReadingView
        openedFromFolder={selectedFolder}
        onAddContact={() => {
          usePeopleStore.getState().requestContactCreate({
            name: snapshot.from_name,
            address: snapshot.from_address,
          });
          useUiStore.getState().setShellView('people');
        }}
      />
    </aside>
  );
}
