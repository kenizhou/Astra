// SPDX-License-Identifier: GPL-3.0-only
// The invitation card: what the open message means to this account's
// calendar, mounted above the reading pane's body. An answerable invitation
// offers its accept, maybe, and decline controls through the respond intent
// and re-pulls the reading pane so the card carries the standing the diary
// now holds; the other kinds render their sentence with no controls; a
// stale copy says so.

import { dispatch, intents } from '@/services/ipc';
import { pullSurface } from '@/events/surfaceEvents';
import type { InvitationBlock, MessageKey } from '@/services/ipc';

interface InvitationCardProps {
  /** The open message the card answers for. */
  message: MessageKey;
  /** The invitation block the snapshot carried. */
  invitation: InvitationBlock;
}

/** The sentence for a standing. */
function standingWord(invitation: InvitationBlock): string {
  if (invitation.stale) {
    return 'This copy is out of date.';
  }
  switch (invitation.response) {
    case 'accepted':
      return 'You have accepted this meeting.';
    case 'tentative':
      return 'You have said maybe to this meeting.';
    case 'declined':
      return 'You have declined this meeting.';
    case 'delegated':
      return 'You have delegated this meeting.';
    default:
      return 'You have not answered yet.';
  }
}

/** The sentence for a kind that offers no answer. */
function kindSentence(kind: InvitationBlock['kind']): string {
  switch (kind) {
    case 'cancelled':
      return 'This meeting was cancelled by the organiser.';
    case 'informational':
      return 'This message is for your information; no answer is wanted.';
    case 'superseded':
      return 'A newer invitation for this meeting exists; this copy is out of date.';
    default:
      return '';
  }
}

/** The invitation card above the reading pane's body. */
export function InvitationCard({ message, invitation }: InvitationCardProps) {
  const answer = (response: 'accepted' | 'tentative' | 'declined'): void => {
    const sent = dispatch(intents.respondToInvitation({ account: message.account, key: message.key }, response));
    // The answer path republishes the calendar, not the reading pane; the
    // card re-reads the pane so the standing it shows is the diary's own.
    void sent.then(
      () => void pullSurface('reading'),
      () => undefined,
    );
  };

  return (
    <section
      aria-label="Invitation"
      className="mx-3 mt-2 rounded-[var(--radius)] border border-border bg-surface px-3 py-2"
    >
      <div className="flex items-baseline justify-between gap-2">
        <h3 className="min-w-0 truncate text-[13px] font-semibold text-foreground">
          {invitation.summary}
        </h3>
        <span className="shrink-0 text-[11px] text-muted-foreground">{invitation.organizer}</span>
      </div>
      {invitation.kind === 'rsvp' ? (
        <>
          <p className="mt-1 text-xs text-muted-foreground">{standingWord(invitation)}</p>
          <div className="mt-2 flex gap-1.5">
            <button
              type="button"
              onClick={() => answer('accepted')}
              className="rounded-[var(--radius)] bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground transition-colors hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Accept
            </button>
            <button
              type="button"
              onClick={() => answer('tentative')}
              className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Maybe
            </button>
            <button
              type="button"
              onClick={() => answer('declined')}
              className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Decline
            </button>
          </div>
        </>
      ) : (
        <p className="mt-1 text-xs text-muted-foreground">{kindSentence(invitation.kind)}</p>
      )}
      {invitation.kind !== 'rsvp' && invitation.stale && (
        <p className="mt-1 text-[11px] text-[var(--warning)]">This copy is out of date.</p>
      )}
    </section>
  );
}
