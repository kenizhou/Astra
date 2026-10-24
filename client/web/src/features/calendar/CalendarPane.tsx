// SPDX-License-Identifier: GPL-3.0-only
// The calendar pane: the page the shell's view switch mounts. The toolbar
// over a resizable three-pane row: the rail (the mini-month navigator and
// the calendar visibility list), the active grid, and the event detail dock
// that picking an event opens. Clicking an event picks it for the dock; the
// dock's Edit opens the event editor, and a drag over empty time opens the
// editor prefilled with the dragged slot. The write banners, the
// reply-delivery prompt, the briefs panel, the device-zone offer prompt,
// and the refresh on entry and on focus regain ride the pane as before.

import { useEffect, useState } from 'react';

import { Group, Panel } from 'react-resizable-panels';

import { dispatch, intents } from '@/services/ipc';
import type { OccurrenceRow } from '@/services/ipc';
import { pct } from '@/components/layout/panelSizes';
import { PaneDivider } from '@/components/layout/paneDivider';

import { AgendaView } from './AgendaView';
import { BriefsPanel } from './BriefsPanel';
import { CalendarRail } from './CalendarRail';
import { CalendarToolbar } from './CalendarToolbar';
import { DayGrid } from './DayGrid';
import { DeviceZoneOffer } from './DeviceZoneOffer';
import { EventDetailDock } from './EventDetailDock';
import { EventDialog } from './EventDialog';
import { MonthGrid } from './MonthGrid';
import { ReplyPromptDialog } from './ReplyPromptDialog';
import { WorkWeekGrid } from './WorkWeekGrid';
import { useCalendarStore } from './calendarStore';
import type { CreateSlot } from './useEventDrag';

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
            className="rounded-[var(--radius-xs)] border border-border-subtle px-2 py-0.5 text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
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

/** One pane card: a subtly bordered surface filling its panel, the corners
 *  barely carved. */
function PaneCard({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col overflow-hidden rounded-md border border-border-subtle bg-card">
      {children}
    </div>
  );
}

/** The calendar page: toolbar, banners, the pane row, and its dialogs. */
export function CalendarPane() {
  const view = useCalendarStore((s) => s.view);
  const replyPrompts = useCalendarStore((s) => s.replyPrompts);
  const selectedEvent = useCalendarStore((s) => s.selectedEvent);
  const setView = useCalendarStore((s) => s.setView);
  const setAnchor = useCalendarStore((s) => s.setAnchor);
  const selectEvent = useCalendarStore((s) => s.selectEvent);
  const clearSelection = useCalendarStore((s) => s.clearSelection);
  const [editing, setEditing] = useState<OccurrenceRow | null>(null);
  const [creating, setCreating] = useState(false);
  const [createSlot, setCreateSlot] = useState<CreateSlot | null>(null);
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

  const openEditor = (row: OccurrenceRow): void => {
    // The editor replaces the dock for the event it edits.
    clearSelection();
    setEditing(row);
    setCreating(false);
  };

  const openCreate = (slot: CreateSlot | null): void => {
    setEditing(null);
    setCreateSlot(slot);
    setCreating(true);
  };

  const pickDay = (dayKey: string): void => {
    setAnchor(dayKey);
    setView('day');
  };

  return (
    <section
      aria-label="Calendar"
      className="relative flex min-w-0 flex-1 flex-col overflow-hidden"
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          clearSelection();
        }
      }}
    >
      <CalendarToolbar onCreate={() => openCreate(null)} onBriefs={() => setBriefsOpen(true)} />
      <DeviceZoneOffer />
      <WriteBanners />
      <div className="min-h-0 flex-1 p-1">
        <Group
          key={selectedEvent !== null ? 'docked' : 'solo'}
          orientation="horizontal"
          className="h-full"
        >
          <Panel id="calendar-rail" defaultSize={pct(20)} minSize={pct(14)}>
            <PaneCard>
              <CalendarRail />
            </PaneCard>
          </Panel>
          <PaneDivider />
          <Panel
            id="calendar-grid"
            defaultSize={pct(selectedEvent !== null ? 54 : 80)}
            minSize={pct(30)}
          >
            <PaneCard>
              {view === 'month' && <MonthGrid onPickDay={pickDay} onSelectEvent={selectEvent} />}
              {view === 'work_week' && (
                <WorkWeekGrid onSelectEvent={selectEvent} onCreateSlot={openCreate} />
              )}
              {view === 'day' && (
                <DayGrid onSelectEvent={selectEvent} onCreateSlot={openCreate} />
              )}
              {view === 'agenda' && <AgendaView onSelectEvent={selectEvent} />}
            </PaneCard>
          </Panel>
          {selectedEvent !== null && <PaneDivider />}
          {selectedEvent !== null && (
            <Panel id="event-dock" defaultSize={pct(26)} minSize={pct(18)}>
              <PaneCard>
                <EventDetailDock onEdit={openEditor} />
              </PaneCard>
            </Panel>
          )}
        </Group>
      </div>
      {(creating || editing !== null) && (
        <EventDialog
          row={editing}
          slot={createSlot}
          onClose={() => {
            setCreating(false);
            setEditing(null);
            setCreateSlot(null);
          }}
        />
      )}
      <BriefsPanel open={briefsOpen} onClose={() => setBriefsOpen(false)} />
      {replyPrompts.length > 0 && <ReplyPromptDialog prompt={replyPrompts[0]!} />}
    </section>
  );
}
