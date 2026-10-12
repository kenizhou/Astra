// SPDX-License-Identifier: GPL-3.0-only
// The briefs panel: the anchored day's meeting brief, asked of the host only
// when the action opens the panel (never before), over the first calendar's
// account, rendering the generated brief with its rows under the clock
// setting, the no-meetings answer, and the failure state a refused
// generation leaves.
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ calendarBriefGenerate: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  calendarBriefGenerate: ipc.calendarBriefGenerate,
}));

import { BriefsPanel } from './BriefsPanel';
import { useCalendarStore } from './calendarStore';
import type { MeetingBriefView } from '@/services/ipc';

const onClose = vi.fn();

beforeEach(() => {
  ipc.calendarBriefGenerate.mockReset();
  onClose.mockReset();
  useCalendarStore.setState({
    calendars: [
      {
        account: 'work',
        id: 'cal-1',
        name: 'Work',
        visible: true,
        can_write: true,
        is_default: true,
      },
    ],
    occurrences: [],
    writeStates: [],
    replyPrompts: [],
    nowData: { now: '2026-10-19T08:12:00Z', zone: 'Europe/Amsterdam' },
    view: 'day',
    anchor: '2026-10-19',
  });
});

afterEach(cleanup);

describe('the briefs panel', () => {
  it('asks for the anchored day over the first calendar account on open', async () => {
    ipc.calendarBriefGenerate.mockResolvedValue({
      account: 'work',
      day: '2026-10-19',
      state: 'no_meetings',
    } satisfies MeetingBriefView);
    render(<BriefsPanel open onClose={onClose} />);

    expect(ipc.calendarBriefGenerate).toHaveBeenCalledWith('work', '2026-10-19');
    await waitFor(() => expect(screen.getByText('No meetings this day.')).toBeTruthy());
  });

  it('renders the generated brief and its meeting rows', async () => {
    ipc.calendarBriefGenerate.mockResolvedValue({
      account: 'work',
      day: '2026-10-19',
      state: 'generated',
      brief: 'The standup opens the day; the planning needs the room confirmed.',
      meetings: [
        {
          title: 'Sprint planning',
          start: '2026-10-19T12:00:00Z',
          end: '2026-10-19T13:00:00Z',
          all_day: false,
          organizer: 'Boss <boss@test.local>',
          attendees: ['Info <info@test.local>'],
          location: 'Room A',
        },
      ],
    } satisfies MeetingBriefView);
    render(<BriefsPanel open onClose={onClose} />);

    await waitFor(() =>
      expect(screen.getByText('The standup opens the day; the planning needs the room confirmed.')).toBeTruthy(),
    );
    expect(screen.getByText('Sprint planning')).toBeTruthy();
    expect(screen.getByText('Room A')).toBeTruthy();
    expect(screen.getByText('Boss <boss@test.local>')).toBeTruthy();
    // 12:00Z to 13:00Z is two to three in the afternoon in Amsterdam; the
    // default clock is 12-hour.
    expect(screen.getByText('2 PM to 3 PM')).toBeTruthy();
  });

  it('renders the failure state a refused generation leaves', async () => {
    ipc.calendarBriefGenerate.mockRejectedValue('the ai surface refused');
    render(<BriefsPanel open onClose={onClose} />);

    await waitFor(() =>
      expect(screen.getByText('The brief could not be generated. Try again.')).toBeTruthy(),
    );
  });

  it('asks again from the Refresh action after a failure', async () => {
    ipc.calendarBriefGenerate.mockRejectedValueOnce('the ai surface refused');
    ipc.calendarBriefGenerate.mockResolvedValue({
      account: 'work',
      day: '2026-10-19',
      state: 'no_meetings',
    } satisfies MeetingBriefView);
    render(<BriefsPanel open onClose={onClose} />);
    await waitFor(() =>
      expect(screen.getByText('The brief could not be generated. Try again.')).toBeTruthy(),
    );

    screen.getByRole('button', { name: 'Try again' }).click();

    await waitFor(() => expect(ipc.calendarBriefGenerate).toHaveBeenCalledTimes(2));
  });

  it('renders nothing when the panel is closed', () => {
    const { container } = render(<BriefsPanel open={false} onClose={onClose} />);

    expect(container.firstChild).toBeNull();
    expect(ipc.calendarBriefGenerate).not.toHaveBeenCalled();
  });
});
