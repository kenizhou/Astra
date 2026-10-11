// SPDX-License-Identifier: GPL-3.0-only
// The invitation card: the block above the reading pane's body. An answerable
// invitation offers its accept, maybe, and decline controls; the other kinds
// render their sentence with no controls; a stale copy says so; and every
// answer re-pulls the reading pane so the card carries the standing the
// diary now holds.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));
const events = vi.hoisted(() => ({ pullSurface: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  pullSurface: events.pullSurface,
}));

import { InvitationCard } from './InvitationCard';
import type { InvitationBlock, MessageKey } from '@/services/ipc';

/** The open message the card answers for. */
const message: MessageKey = { account: 'work', key: 'imap:v1:u7@INBOX' };

/** An invitation block, overridable per test. */
function invitation(over: Partial<InvitationBlock> = {}): InvitationBlock {
  return {
    kind: 'rsvp',
    response: 'needs_action',
    stale: false,
    summary: 'Sprint planning',
    organizer: 'boss@example.test',
    ...over,
  };
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  events.pullSurface.mockReset();
  events.pullSurface.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe('the invitation card', () => {
  it('offers the three answers on an answerable invitation', () => {
    render(<InvitationCard message={message} invitation={invitation()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    fireEvent.click(screen.getByRole('button', { name: 'Maybe' }));
    fireEvent.click(screen.getByRole('button', { name: 'Decline' }));

    const sent = ipc.dispatch.mock.calls.map((call) => JSON.stringify(call[0]));
    expect(sent).toEqual([
      '{"calendar":{"respond_to_invitation":{"message":{"account":"work","key":"imap:v1:u7@INBOX"},"response":"accepted","notify_organizer":true}}}',
      '{"calendar":{"respond_to_invitation":{"message":{"account":"work","key":"imap:v1:u7@INBOX"},"response":"tentative","notify_organizer":true}}}',
      '{"calendar":{"respond_to_invitation":{"message":{"account":"work","key":"imap:v1:u7@INBOX"},"response":"declined","notify_organizer":true}}}',
    ]);
  });

  it('re-pulls the reading pane after an answer so the card updates', async () => {
    render(<InvitationCard message={message} invitation={invitation()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
    await Promise.resolve();

    expect(events.pullSurface).toHaveBeenCalledWith('reading');
  });

  it('names the meeting and the organiser beside the standing', () => {
    render(
      <InvitationCard message={message} invitation={invitation({ response: 'accepted' })} />,
    );

    expect(screen.getByText('Sprint planning')).toBeTruthy();
    expect(screen.getByText('boss@example.test')).toBeTruthy();
    expect(screen.getByText('You have accepted this meeting.')).toBeTruthy();
  });

  it('says the hold when the meeting is unanswered', () => {
    render(<InvitationCard message={message} invitation={invitation()} />);

    expect(screen.getByText('You have not answered yet.')).toBeTruthy();
  });

  it('sentences a cancellation with no controls', () => {
    render(
      <InvitationCard message={message} invitation={invitation({ kind: 'cancelled' })} />,
    );

    expect(screen.getByText('This meeting was cancelled by the organiser.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('sentences an informational copy with no controls', () => {
    render(
      <InvitationCard message={message} invitation={invitation({ kind: 'informational' })} />,
    );

    expect(
      screen.getByText('This message is for your information; no answer is wanted.'),
    ).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
  });

  it('sentences a superseded copy without repeating the stale line', () => {
    render(
      <InvitationCard
        message={message}
        invitation={invitation({ kind: 'superseded', stale: true })}
      />,
    );

    expect(
      screen.getByText('A newer invitation for this meeting exists; this copy is out of date.'),
    ).toBeTruthy();
    // The sentence already says it: the generic stale line stays unsaid.
    expect(screen.queryByText('This copy is out of date.')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Accept' })).toBeNull();
  });

  it('says so when a copy of another kind is stale', () => {
    render(
      <InvitationCard message={message} invitation={invitation({ kind: 'cancelled', stale: true })} />,
    );

    expect(screen.getByText('This copy is out of date.')).toBeTruthy();
  });
});
