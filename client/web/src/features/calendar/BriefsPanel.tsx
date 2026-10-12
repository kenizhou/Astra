// SPDX-License-Identifier: GPL-3.0-only
// The briefs panel: the anchored day's meeting brief over the first
// calendar's account, asked of the host only when the action opens the
// panel (the brief is a paid call; nothing generates before the user asks),
// rendering the generated brief with its meeting rows under the clock
// setting, the no-meetings answer, and the failure state a refused
// generation leaves.

import { useCallback, useEffect, useState } from 'react';

import { calendarBriefGenerate } from '@/services/ipc';
import type { MeetingBriefView } from '@/services/ipc';

import { useCalendarStore } from './calendarStore';
import { dayKeyOf, minutesInZone } from './calendarRange';
import { clockLabel } from './dayLabels';
import { useSettingsStore } from '@/features/settings/settingsStore';

type BriefState =
  | { kind: 'asking' }
  | { kind: 'ready'; view: MeetingBriefView }
  | { kind: 'failed' };

interface BriefsPanelProps {
  /** Whether the panel is up. */
  open: boolean;
  /** Closes the panel. */
  onClose: () => void;
}

/** One meeting row of the brief. */
function MeetingRow({ row, zone }: { row: NonNullable<MeetingBriefView['meetings']>[number]; zone: string }) {
  const twentyFour = useSettingsStore((s) => s.settings.twenty_four_clock);
  return (
    <li className="flex flex-col gap-0.5 rounded-[var(--radius-xs)] border border-border px-2 py-1.5">
      <span className="flex items-center gap-2 text-xs font-medium text-foreground">
        <span className="tabular-nums text-muted-foreground">
          {row.all_day
            ? 'All day'
            : `${clockLabel(minutesInZone(new Date(row.start), zone), twentyFour)} to ${clockLabel(
                minutesInZone(new Date(row.end), zone),
                twentyFour,
              )}`}
        </span>
        <span className="min-w-0 flex-1 truncate">{row.title}</span>
      </span>
      {(row.organizer !== undefined || row.location !== undefined) && (
        <span className="flex flex-wrap gap-1 text-[11px] text-muted-foreground">
          {row.organizer !== undefined && <span>{row.organizer}</span>}
          {row.organizer !== undefined && row.location !== undefined && <span>·</span>}
          {row.location !== undefined && <span>{row.location}</span>}
        </span>
      )}
    </li>
  );
}

/** The briefs panel over the anchored day. */
export function BriefsPanel({ open, onClose }: BriefsPanelProps) {
  const calendars = useCalendarStore((s) => s.calendars);
  const anchor = useCalendarStore((s) => s.anchor);
  const nowData = useCalendarStore((s) => s.nowData);
  const zone = nowData?.zone ?? 'Etc/UTC';
  const [state, setState] = useState<BriefState>({ kind: 'asking' });

  const account = calendars[0]?.account;
  const day = anchor ?? (nowData !== null ? dayKeyOf(new Date(nowData.now), zone) : null);

  const ask = useCallback(async () => {
    if (account === null || account === undefined || day === null) {
      return;
    }
    setState({ kind: 'asking' });
    try {
      const view = await calendarBriefGenerate(account, day);
      setState({ kind: 'ready', view });
    } catch {
      setState({ kind: 'failed' });
    }
  }, [account, day]);

  useEffect(() => {
    if (open) {
      void ask();
    }
  }, [open, ask]);

  if (!open) {
    return null;
  }

  return (
    <section
      aria-label="Briefs"
      className="absolute bottom-[var(--status-h)] right-3 z-[var(--z-modal)] flex max-h-[70%] w-[380px] flex-col overflow-hidden rounded-[var(--radius)] border border-border bg-surface-floating shadow-[var(--shadow-lg)]"
    >
      <header className="flex shrink-0 items-center justify-between border-b border-border px-3 py-2">
        <h2 className="text-xs font-semibold text-foreground">Meeting brief</h2>
        <button
          type="button"
          aria-label="Close briefs"
          onClick={onClose}
          className="rounded-[var(--radius-xs)] p-1 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground"
        >
          ×
        </button>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto p-3 text-xs">
        {state.kind === 'asking' && <p className="text-muted-foreground">Generating the brief…</p>}
        {state.kind === 'failed' && (
          <div className="flex flex-col gap-2">
            <p className="text-foreground">The brief could not be generated. Try again.</p>
            <button
              type="button"
              onClick={() => void ask()}
              className="self-start rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-foreground transition-colors hover:bg-hover"
            >
              Try again
            </button>
          </div>
        )}
        {state.kind === 'ready' && state.view.state === 'no_meetings' && (
          <p className="text-muted-foreground">No meetings this day.</p>
        )}
        {state.kind === 'ready' && state.view.state === 'generated' && (
          <div className="flex flex-col gap-2">
            <p className="whitespace-pre-wrap text-foreground">{state.view.brief}</p>
            {state.view.meetings !== undefined && (
              <ul className="flex flex-col gap-1">
                {state.view.meetings.map((meeting) => (
                  <MeetingRow key={`${meeting.title}:${meeting.start}`} row={meeting} zone={zone} />
                ))}
              </ul>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
