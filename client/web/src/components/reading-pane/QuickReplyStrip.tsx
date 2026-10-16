// SPDX-License-Identifier: GPL-3.0-only
// The quick-reply strip: docked at the reading pane's bottom, kylins'
// InlineReply idiom. Collapsed it is the three compose verbs as one-line
// affordances, shown only while a message is open; a click flags the next
// compose request for the inline route and dispatches the verb, so the
// core's prefill and threading land in the docked composer rather than a
// pop-out. Expanded it is the shared composer pane, sending through the
// same submit path; the pane leaving (a send, a discard) collapses the
// strip back to the affordances.

import { ComposerPane } from '@/features/composer/Composer';
import { useComposerStore } from '@/features/composer/composerStore';
import { ForwardIcon, ReplyAllIcon, ReplyIcon } from '@/components/icons';
import { dispatch, intents, type Intent } from '@/services/ipc';
import { useReadingStore } from '@/stores/readingStore';

/** One affordance's click: the next landed request opens inline, on this
 *  message, with the verb it names. */
function startQuickCompose(verb: () => Intent): void {
  const message = useReadingStore.getState().snapshot?.message;
  if (message === undefined) {
    return;
  }
  useComposerStore.getState().expectInlineReply();
  void dispatch(verb());
}

const VERBS = [
  { label: 'Reply', icon: ReplyIcon, intent: () => intents.replyTo(messageOf()) },
  { label: 'Reply all', icon: ReplyAllIcon, intent: () => intents.replyAllTo(messageOf()) },
  { label: 'Forward', icon: ForwardIcon, intent: () => intents.forward(messageOf()) },
] as const;

/** The open message's ref; the verbs are only rendered while one stands. */
function messageOf() {
  return useReadingStore.getState().snapshot!.message;
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
    <div className="flex shrink-0 items-center gap-2 border-t border-border-subtle px-7 py-2">
      {VERBS.map(({ label, icon: Icon, intent }) => (
        <button
          key={label}
          type="button"
          onClick={() => startQuickCompose(intent)}
          disabled={expecting}
          className="flex flex-1 items-center justify-center gap-2 rounded-full border border-border bg-background px-3 py-1.5 text-[13px] text-muted-foreground transition-colors hover:border-ring hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-60"
        >
          <Icon size={15} />
          {expecting ? 'Preparing' : label}
        </button>
      ))}
    </div>
  );
}
