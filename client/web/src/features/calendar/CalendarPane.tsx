// SPDX-License-Identifier: GPL-3.0-only
// The calendar pane: the page the shell's view switch mounts. The toolbar
// over the active view, the write banners the write states draw, the
// reply-delivery prompt riding above the view while one stands, the event
// editor the rows and the New event action open, the briefs panel, and the
// refresh on entry and on focus regain.

import { useEffect, useState } from 'react';

import { dispatch, intents } from '@/services/ipc';
import type { OccurrenceRow } from '@/services/ipc';

import { AgendaView } from './AgendaView';
import { BriefsPanel } from './BriefsPanel';
import { CalendarToolbar } from './CalendarToolbar';
import { DayGrid } from './DayGrid';
import { EventDialog } from './EventDialog';
import { MonthGrid } from './MonthGrid';
import { ReplyPromptDialog } from './ReplyPromptDialog';
import { WorkWeekGrid } from './WorkWeekGrid';
import { useCalendarStore } from './calendarStore';

/** The write banners: a write settling, and a write that did not settle. */
function WriteBanners() {
  const writeStates = useCalendarStore((s) => s.writeStates);
  const failed = writeStates.some((row) => row.state === 'failed_with_retry');
  const writing = writeStates.some((row) => row.state === 'writing');

  if (!writing && !failed) {
    return null;
  }
  return (
    <div
      role="status"
      className={`flex shrink-0 items-center justify-between gap-2 border-b px-3 py-1.5 text-xs ${
        failed
          ? 'border-[var(--warning)]/40 bg-[var(--warning)]/10 text-warning-foreground'
          : 'border-border-subtle bg-surface text-muted-foreground'
      }`}
    >
      {failed ? (
        <>
          <span>The last calendar change did not settle. Refresh to see what stood.</span>
          <button
            type="button"
            aria-label="Retry"
            onClick={() => void dispatch(intents.refreshCalendar())}
            className="rounded-[var(--radius-xs)] border border-border-subtle px-2 py-0.5 text-xs text-foreground transition-colors hover:bg-hover"
          >
            Retry
          </button>
        </>
      ) : (
        <span className="inline-flex items-center gap-1.5">
          <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-current border-t-transparent" />
          Saving calendar changes…
        </span>
      )}
    </div>
  );
}

/** The calendar page: toolbar, banners, the active view, and its dialogs. */
export function CalendarPane() {
  const view = useCalendarStore((s) => s.view);
  const replyPrompts = useCalendarStore((s) => s.replyPrompts);
  const setView = useCalendarStore((s) => s.setView);
  const setAnchor = useCalendarStore((s) => s.setAnchor);
  const [editing, setEditing] = useState<OccurrenceRow | null>(null);
  const [creating, setCreating] = useState(false);
  const [briefsOpen, setBriefsOpen] = useState(false);

  // A calendar entering the view syncs: the same refresh a failed write's
  // recovery waits for, and the one a focus regain repeats.
  useEffect(() => {
    void dispatch(intents.refreshCalendar());
    const onFocus = (): void => {
      void dispatch(intents.refreshCalendar());
    };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);

  const openEvent = (row: OccurrenceRow): void => {
    setEditing(row);
    setCreating(false);
  };

  const pickDay = (dayKey: string): void => {
    setAnchor(dayKey);
    setView('day');
  };

  return (
    <section
      aria-label="Calendar"
      className="relative flex min-w-0 flex-1 flex-col overflow-hidden"
    >
      <CalendarToolbar onCreate={() => setCreating(true)} onBriefs={() => setBriefsOpen(true)} />
      <WriteBanners />
      {view === 'month' && <MonthGrid onPickDay={pickDay} onOpenEvent={openEvent} />}
      {view === 'work_week' && <WorkWeekGrid onOpenEvent={openEvent} />}
      {view === 'day' && <DayGrid onOpenEvent={openEvent} />}
      {view === 'agenda' && <AgendaView onOpenEvent={openEvent} />}
      {(creating || editing !== null) && (
        <EventDialog row={editing} onClose={() => { setCreating(false); setEditing(null); }} />
      )}
      <BriefsPanel open={briefsOpen} onClose={() => setBriefsOpen(false)} />
      {replyPrompts.length > 0 && <ReplyPromptDialog prompt={replyPrompts[0]!} />}
    </section>
  );
}
