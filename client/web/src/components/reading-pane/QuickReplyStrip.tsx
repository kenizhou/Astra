// SPDX-License-Identifier: GPL-3.0-only
// The quick-reply strip: docked at the reading pane's bottom, kylins'
// InlineReply idiom. Collapsed it is a one-line Reply affordance, shown only
// while a message is open; the click flags the next compose request for the
// inline route and dispatches the reply, so the core's prefill and threading
// land in the docked composer rather than a pop-out. Expanded it is the
// shared composer pane, sending through the same submit path; the pane
// leaving (a send, a discard) collapses the strip back to the affordance.

import { ComposerPane } from '@/features/composer/Composer';
import { useComposerStore } from '@/features/composer/composerStore';
import { ReplyIcon } from '@/components/icons';
import { dispatch, intents } from '@/services/ipc';
import { useReadingStore } from '@/stores/readingStore';

/** The strip's click: the next landed request opens inline, on this message. */
function startQuickReply(): void {
  const message = useReadingStore.getState().snapshot?.message;
  if (message === undefined) {
    return;
  }
  useComposerStore.getState().expectInlineReply();
  void dispatch(intents.replyTo(message));
}

export function QuickReplyStrip() {
  const open = useComposerStore((s) => s.open && s.composition !== null);
  const messageOpen = useReadingStore((s) => s.snapshot !== null);
  const expecting = useComposerStore((s) => s.inlineExpected);

  if (open) {
    return (
      <div className="flex max-h-[55%] min-h-[220px] shrink-0 flex-col border-t border-border-subtle">
        <ComposerPane />
      </div>
    );
  }

  if (!messageOpen) {
    return null;
  }

  return (
    <div className="shrink-0 border-t border-border-subtle px-7 py-2">
      <button
        type="button"
        onClick={startQuickReply}
        disabled={expecting}
        className="flex w-full items-center gap-2 rounded-[var(--radius-sm)] border border-border bg-background px-3 py-2 text-left text-[13px] text-muted-foreground transition-colors hover:border-ring hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-60"
      >
        <ReplyIcon size={15} />
        {expecting ? 'Preparing the reply' : 'Reply'}
      </button>
    </div>
  );
}
