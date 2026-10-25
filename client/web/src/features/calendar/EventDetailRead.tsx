// SPDX-License-Identifier: GPL-3.0-only
// The event detail read the surfaces share: the hook that pulls one
// event's detail over the wire by its reference, the roll-call (the
// organiser first, each participant's name-or-address with the word for
// their seat and their answer, the tail the wire folded into a count
// sentence), and the repeating rule's read-back sentence. The dock renders
// these beside its affordances; the editor renders them read-only so the
// person can see who they are writing to. Neither can change a guest list
// this slice: the list is the read, not an edit.

import { useEffect, useState } from 'react';

import { calendarEventDetail } from '@/services/ipc';
import type { EventDetail, OccurrenceRow, Participation } from '@/services/ipc';

/** The answer words the roll-call renders, one per wire state. */
const RESPONSE_WORDS: Record<Participation, string> = {
  accepted: 'accepted',
  tentative: 'tentative',
  declined: 'declined',
  delegated: 'delegated',
  needs_action: 'not answered yet',
};

/**
 * Pulls the detail of the event `row` names, keying on the reference rather
 * than the row object so a publish that replaces the row with its own fresh
 * copy does not re-ask. Answers null until the read lands (or for ever,
 * when nobody holds the event); the caller keeps rendering its own rows.
 */
export function useEventDetail(row: OccurrenceRow | null): EventDetail | null {
  const [detail, setDetail] = useState<EventDetail | null>(null);
  const account = row?.account;
  const event = row?.event;
  useEffect(() => {
    setDetail(null);
    if (account === undefined || event === undefined) {
      return;
    }
    let standing = true;
    calendarEventDetail(account, event)
      .then((read) => {
        // A refused or empty read leaves the detail standing empty; only a
        // read that carries the detail's own shape lands.
        if (standing && read !== null && read !== undefined) {
          setDetail(read);
        }
      })
      .catch(() => {
        // An unresolvable read leaves the detail empty; the surface's own
        // rows still render.
      });
    return () => {
      standing = false;
    };
  }, [account, event]);
  return detail;
}

/**
 * The roll-call: one line per participant the wire carried, the seat word
 * where the participant holds one beside the plain attendee's, the answer
 * always, and the fold count sentence where the wire folded the tail.
 * Renders nothing for an event that carries nobody.
 */
export function RollCall({ detail }: { detail: EventDetail }) {
  const participants = detail.participants ?? [];
  if (participants.length === 0) {
    return null;
  }
  return (
    <div className="flex flex-col gap-0.5">
      {participants.map((participant) => (
        <p key={participant.name_or_address} className="text-[13px] text-foreground">
          {participant.name_or_address}{' '}
          <span className="text-[11px] text-muted-foreground">
            {participant.role !== undefined ? `${participant.role}, ` : ''}
            {RESPONSE_WORDS[participant.response]}
          </span>
        </p>
      ))}
      {(detail.folded_participants ?? 0) > 0 && (
        <p className="text-[11px] text-muted-foreground">
          and {detail.folded_participants} more people
        </p>
      )}
    </div>
  );
}

/**
 * The rule read-back: the sentence the core worded from the rule's own
 * shape. Renders nothing for a one-off or a rule the read did not carry.
 */
export function RuleSentence({ detail }: { detail: EventDetail }) {
  if (detail.rule === undefined) {
    return null;
  }
  return <span className="text-[13px] text-foreground">{detail.rule.sentence}</span>;
}

/**
 * The editor's read-only copy of the detail: the rule sentence where the
 * event repeats and the roll-call where it carries guests, in the dialog's
 * own chrome. Renders nothing for an event that carries neither.
 */
export function DetailReadonlyCopy({
  detail,
  recurring,
}: {
  detail: EventDetail;
  recurring: boolean;
}) {
  const attendees = (detail.participants ?? []).length > 0;
  if (!recurring && !attendees) {
    return null;
  }
  return (
    <div className="flex flex-col gap-2">
      {recurring && detail.rule !== undefined && (
        <p className="text-xs text-muted-foreground">
          <RuleSentence detail={detail} />
        </p>
      )}
      {attendees && (
        <div className="flex flex-col gap-1">
          <span className="text-xs text-muted-foreground">Attendees</span>
          <RollCall detail={detail} />
        </div>
      )}
    </div>
  );
}
