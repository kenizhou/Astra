// SPDX-License-Identifier: GPL-3.0-only
// The reply-delivery prompt: the answer that landed on the calendar but
// whose telling of the organiser failed. The plain-language sentence, the
// note the reply carries, the send and don't-send decisions, the remember
// checkbox that maps onto the account's standing preference, and the subject
// override when the host composed a localised one.

import { useState } from 'react';

import { dispatch, intents } from '@/services/ipc';
import type { ReplyPromptRow } from '@/services/ipc';

interface ReplyPromptDialogProps {
  /** The standing prompt, the most recently raised one. */
  prompt: ReplyPromptRow;
}

/** The word for an answer, in the plain-language sentence. */
function answerWord(response: ReplyPromptRow['response']): string {
  switch (response) {
    case 'accepted':
      return 'accepted';
    case 'tentative':
      return 'said maybe to';
    case 'declined':
      return 'declined';
    default:
      return 'answered';
  }
}

/** The reply-delivery-failure prompt. */
export function ReplyPromptDialog({ prompt }: ReplyPromptDialogProps) {
  const [remember, setRemember] = useState(false);
  const [subject, setSubject] = useState('');

  const answer = (send: boolean): void => {
    const override = subject.trim() === '' ? undefined : subject.trim();
    void dispatch(intents.answerReplyPrompt(send, remember, override));
  };

  return (
    <div className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4">
      <div
        role="dialog"
        aria-label="Reply not delivered"
        className="flex w-[420px] flex-col gap-3 rounded-[var(--radius)] border border-border bg-surface-floating p-4 shadow-[var(--shadow-lg)]"
      >
        <h2 className="text-sm font-semibold text-foreground">{prompt.summary}</h2>
        <p className="text-xs text-foreground">
          You {answerWord(prompt.response)} this meeting, but{' '}
          <span className="font-medium">{prompt.organizer}</span> has not heard: the calendar
          server could not deliver the reply.
        </p>
        <p className="text-xs text-muted-foreground">
          Your answer reached the calendar, but the server could not tell the organiser.
        </p>
        {prompt.comment !== undefined && (
          <label className="flex flex-col gap-1 text-xs text-muted-foreground">
            The note your reply carries
            <textarea
              aria-label="The note your reply carries"
              value={prompt.comment}
              readOnly
              className="min-h-[48px] w-full rounded-[var(--radius-xs)] border border-input bg-surface px-2 py-1 text-xs text-foreground"
            />
          </label>
        )}
        <label className="flex flex-col gap-1 text-xs text-muted-foreground">
          Subject (optional override)
          <input
            aria-label="Subject"
            type="text"
            value={subject}
            onChange={(event) => setSubject(event.target.value)}
            placeholder="Re: the invitation's own subject"
            className="h-7 w-full rounded-[var(--radius-xs)] border border-input bg-background px-2 text-xs text-foreground outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring"
          />
        </label>
        <label className="flex items-center gap-2 text-xs text-foreground">
          <input
            aria-label="Remember this decision"
            type="checkbox"
            checked={remember}
            onChange={(event) => setRemember(event.target.checked)}
          />
          Remember this decision for this account
        </label>
        <div className="flex justify-end gap-2 border-t border-border pt-3">
          <button
            type="button"
            onClick={() => answer(false)}
            className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground"
          >
            Don't send
          </button>
          <button
            type="button"
            onClick={() => answer(true)}
            className="rounded-[var(--radius)] bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground transition-colors hover:opacity-90"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
