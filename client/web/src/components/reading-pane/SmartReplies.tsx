// SPDX-License-Identifier: GPL-3.0-only
// The smart replies strip: the open message's chips, each its one-line
// preview beside the accept affordance that opens the reply composer with
// the drafted reply already the document, and the one dismissal that hides
// the message's chips for good. The strip stands absent with nothing open or
// no chips the gate cleared; the pull and the draft-pool re-pull live in the
// ai chips store.

import { useEffect } from 'react';

import { AiChip } from '@/components/ui/AiChip';
import { composeFromDraft } from '@/services/ai';
import { useAiChipsStore } from '@/stores/aiChipsStore';
import { useReadingStore } from '@/stores/readingStore';

/** A stroke-drawn cross for the strip's dismissal. */
function CrossGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="11"
      height="11"
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

/** The shared classes of the strip's two stroke-drawn buttons. */
const GLYPH_BUTTON =
  'rounded-[var(--radius-xs)] p-0.5 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

export function SmartReplies() {
  const message = useReadingStore((s) => s.snapshot?.message ?? null);
  const chips = useAiChipsStore((s) => s.chips);
  const account = message?.account;
  const key = message?.key;

  useEffect(() => {
    if (account === undefined || key === undefined) {
      return;
    }
    useAiChipsStore.getState().open({ account, key });
    return () => useAiChipsStore.getState().clear();
  }, [account, key]);

  if (message === null || chips.length === 0) {
    return null;
  }

  return (
    <section
      aria-label="Suggested replies"
      className="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-border px-4 py-2"
    >
      <span className="text-xs text-muted-foreground">Suggested replies</span>
      {chips.map((chip, index) => (
        <AiChip
          key={`${chip.preview}-${index}`}
          label={chip.preview}
          provenance="from this thread"
          state="approve"
          onApprove={() => void composeFromDraft(message, chip.reply)}
          approveLabel={`Reply with: ${chip.preview}`}
        />
      ))}
      <button
        type="button"
        aria-label="Dismiss suggested replies"
        onClick={() => useAiChipsStore.getState().dismiss(message)}
        className={`ml-auto ${GLYPH_BUTTON}`}
      >
        <CrossGlyph />
      </button>
    </section>
  );
}
