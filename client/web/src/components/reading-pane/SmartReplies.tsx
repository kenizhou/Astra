// SPDX-License-Identifier: GPL-3.0-only
// The smart replies strip: the open message's chips, each its one-line
// preview beside the accept affordance that opens the reply composer with
// the drafted reply already the document, and the one dismissal that hides
// the message's chips for good. The strip stands absent with nothing open or
// no chips the gate cleared; the pull and the draft-pool re-pull live in the
// ai chips store.

import { useEffect } from 'react';

import { composeFromDraft } from '@/services/ai';
import { useAiChipsStore } from '@/stores/aiChipsStore';
import { useReadingStore } from '@/stores/readingStore';

/** A stroke-drawn reply arrow for a chip's accept affordance. */
function ReplyGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="12"
      height="12"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M9 14 4 9l5-5" />
      <path d="M4 9h10a6 6 0 0 1 6 6v4" />
    </svg>
  );
}

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
      aria-label="Smart replies"
      className="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-border px-4 py-2"
    >
      <span className="text-xs text-muted-foreground">Suggested replies</span>
      {chips.map((chip, index) => (
        <div
          key={`${chip.preview}-${index}`}
          className="flex items-center gap-1 rounded-[var(--radius)] border border-border px-2 py-1"
        >
          <span className="max-w-[180px] truncate text-xs text-foreground">{chip.preview}</span>
          <button
            type="button"
            aria-label={`Reply with: ${chip.preview}`}
            onClick={() => void composeFromDraft(message, chip.reply)}
            className={GLYPH_BUTTON}
          >
            <ReplyGlyph />
          </button>
        </div>
      ))}
      <button
        type="button"
        aria-label="Dismiss smart replies"
        onClick={() => useAiChipsStore.getState().dismiss(message)}
        className={`ml-auto ${GLYPH_BUTTON}`}
      >
        <CrossGlyph />
      </button>
    </section>
  );
}
