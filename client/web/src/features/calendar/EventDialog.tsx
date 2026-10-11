// SPDX-License-Identifier: GPL-3.0-only
// The event dialog: creation over the anchor day, and the edit split the
// wire's own shapes force. A content edit crosses through the update intent;
// a time edit crosses through the drag currency, whose wall-clock delta is
// the same in any zone, so an editor that shows the display zone's clock
// never mistranslates a zoned event it cannot see the zone of. One-vs-all
// scopes a recurring row; a series edit that would discard per-occurrence
// changes is confirmed first; the repeat rule is read-only (the snapshot
// carries no rule to seed an editor with, and a rule half-seen is a rule
// silently rewritten).

import { useMemo, useState } from 'react';

import { dispatch, intents } from '@/services/ipc';
import type {
  CalendarRef,
  EventDrag,
  Frequency,
  OccurrenceRow,
} from '@/services/ipc';

import { addDays, dayKeyOf, minutesInZone } from './calendarRange';
import { useCalendarStore } from './calendarStore';
import { DeleteChoice, SeriesWarning } from './EventDialogSteps';

/** The repeat choices a create offers. */
const REPEATS: ReadonlyArray<{ id: Frequency | 'none'; label: string }> = [
  { id: 'none', label: 'Does not repeat' },
  { id: 'daily', label: 'Daily' },
  { id: 'weekly', label: 'Weekly' },
  { id: 'monthly', label: 'Monthly' },
  { id: 'yearly', label: 'Yearly' },
];

/** A datetime-local value as a civil day and minute. */
function parseLocal(text: string): { day: string; minutes: number } {
  const [date, clock] = text.split('T');
  const [hour, minute] = (clock ?? '00:00').split(':').map(Number);
  return { day: date!, minutes: hour! * 60 + minute! };
}

/** A civil day as its UTC day number, for day deltas. */
function dayNumber(day: string): number {
  const [year, month, date] = day.split('-').map(Number);
  return Math.floor(Date.UTC(year!, month! - 1, date!) / 86_400_000);
}

/** The wall-clock delta between two local values, unsnapped. */
function localDelta(from: string, to: string): { days: number; minutes: number } {
  const a = parseLocal(from);
  const b = parseLocal(to);
  return { days: dayNumber(b.day) - dayNumber(a.day), minutes: b.minutes - a.minutes };
}

/** A zone-local clock of an instant as a datetime-local value. */
function localValueOf(instant: string, zone: string): string {
  const at = new Date(instant);
  const day = dayKeyOf(at, zone);
  const minutes = minutesInZone(at, zone);
  const hh = String(Math.floor(minutes / 60)).padStart(2, '0');
  const mm = String(minutes % 60).padStart(2, '0');
  return `${day}T${hh}:${mm}`;
}

/** The field chrome every input shares. */
function fieldClass(): string {
  return 'h-7 w-full rounded-[var(--radius-xs)] border border-input bg-background px-2 text-xs text-foreground outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring';
}

interface EventDialogProps {
  /** The occurrence being edited, or null to create. */
  row: OccurrenceRow | null;
  /** Closes the dialog. */
  onClose: () => void;
}

/** The create and edit dialog over one event. */
export function EventDialog({ row, onClose }: EventDialogProps) {
  const calendars = useCalendarStore((s) => s.calendars);
  const anchor = useCalendarStore((s) => s.anchor);
  const nowData = useCalendarStore((s) => s.nowData);
  const zone = nowData?.zone ?? 'Etc/UTC';

  const writable = useMemo(
    () => calendars.filter((calendar) => calendar.can_write),
    [calendars],
  );
  const defaultCalendar = writable.find((calendar) => calendar.is_default) ?? writable[0];

  const editing = row !== null;
  const scopeable = editing && row.recurring && row.occurrence_start !== undefined;

  const [summary, setSummary] = useState(row?.title ?? '');
  const [location, setLocation] = useState('');
  const [notes, setNotes] = useState('');
  const [calendarKey, setCalendarKey] = useState(
    editing
      ? ''
      : defaultCalendar !== undefined
        ? `${defaultCalendar.account}:${defaultCalendar.id}`
        : '',
  );
  const day = row !== null ? dayKeyOf(new Date(row.start), zone) : (anchor ?? '2026-01-01');
  const [allDay, setAllDay] = useState(row?.all_day ?? false);
  const [startText, setStartText] = useState(
    row !== null ? localValueOf(row.start, zone) : `${day}T09:00`,
  );
  const [endText, setEndText] = useState(
    row !== null ? localValueOf(row.end, zone) : `${day}T10:00`,
  );
  const [dateText, setDateText] = useState(day);
  const [repeat, setRepeat] = useState<Frequency | 'none'>('none');
  const [endsAfter, setEndsAfter] = useState('');
  const [scope, setScope] = useState<'this' | 'all'>(scopeable ? 'this' : 'all');
  const [confirmSeries, setConfirmSeries] = useState(false);
  const [deleteChoice, setDeleteChoice] = useState(false);

  const title = editing ? 'Edit event' : 'New event';
  const eventRef = editing ? { account: row.account, key: row.event } : null;
  const thisScope = scopeable && scope === 'this';

  /** The content fields, disabled in a one-occurrence scope: a title belongs
   * to the series, and the occurrence edit the wire takes cannot carry one. */
  const contentDisabled = thisScope;

  const save = (confirmed = false): void => {
    if (row === null) {
      create();
      return;
    }
    // A series edit that the row warns about is confirmed before it crosses.
    if (row.series_edit_warns && !thisScope && !confirmed) {
      setConfirmSeries(true);
      return;
    }
    const edit: { title?: string; notes?: string; location?: string } = {};
    if (!contentDisabled && summary !== row.title && summary.trim() !== '') {
      edit.title = summary.trim();
    }
    if (!contentDisabled && location.trim() !== '') {
      edit.location = location.trim();
    }
    if (!contentDisabled && notes.trim() !== '') {
      edit.notes = notes.trim();
    }
    if (Object.keys(edit).length > 0) {
      void dispatch(intents.updateCalendarEvent({ account: row.account, key: row.event }, edit));
    }
    const startChanged = startText !== localValueOf(row.start, zone);
    const endChanged = endText !== localValueOf(row.end, zone);
    if (allDay) {
      const moved = dateText !== day;
      if (moved && eventRef !== null) {
        const delta = localDelta(`${day}T00:00`, `${dateText}T00:00`);
        move(delta.days, delta.minutes);
      }
    } else if ((startChanged || endChanged) && eventRef !== null) {
      // Both edges shift by the start's delta, the same whole-move the grid
      // drag makes; a resize is the drag gesture's own affordance.
      const delta = localDelta(localValueOf(row.start, zone), startText);
      move(delta.days, delta.minutes);
    }
    onClose();
  };

  /** Sends the move intent, scoped to the one occurrence when it stands. */
  const move = (days: number, minutes: number): void => {
    if (eventRef === null || row === null) {
      return;
    }
    const drag: EventDrag = {
      edge: 'whole',
      days,
      minutes,
      ...(thisScope ? { occurrence: row.occurrence_start } : {}),
    };
    void dispatch(intents.moveCalendarEvent(eventRef, drag));
  };

  const create = (): void => {
    if (defaultCalendar === undefined && calendarKey === '') {
      return;
    }
    const chosen: CalendarRef | undefined =
      calendarKey === ''
        ? defaultCalendar !== undefined
          ? { account: defaultCalendar.account, key: defaultCalendar.id }
          : undefined
        : (() => {
            const [account, key] = calendarKey.split(':');
            return account !== undefined && key !== undefined ? { account, key } : undefined;
          })();
    if (chosen === undefined) {
      return;
    }
    const recurrence =
      repeat === 'none'
        ? undefined
        : {
            frequency: repeat,
            interval: 1,
            weekdays: [],
            end: endsAfter.trim() === '' ? ('never' as const) : { after_count: Number(endsAfter) },
          };
    void dispatch(
      intents.createCalendarEvent({
        calendar: chosen,
        summary: summary.trim(),
        start: allDay
          ? { Date: dateText }
          : { Zoned: { local: `${startText}:00`, zone: { Iana: zone } } },
        end: allDay
          ? { Date: addDays(dateText, 1) }
          : { Zoned: { local: `${endText}:00`, zone: { Iana: zone } } },
        ...(location.trim() !== '' && { location: location.trim() }),
        ...(notes.trim() !== '' && { description: notes.trim() }),
        ...(recurrence !== undefined && { recurrence }),
      }),
    );
    onClose();
  };

  const remove = (occurrence: boolean): void => {
    if (row === null) {
      return;
    }
    void dispatch(
      intents.deleteCalendarEvent(
        { account: row.account, key: row.event },
        occurrence ? row.occurrence_start : undefined,
      ),
    );
    onClose();
  };

  return (
    <div className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4">
      <div
        role="dialog"
        aria-label={title}
        className="flex max-h-full w-[420px] flex-col overflow-y-auto rounded-[var(--radius)] border border-border bg-surface-floating p-4 shadow-[var(--shadow-lg)]"
      >
        <h2 className="mb-3 text-sm font-semibold text-foreground">{title}</h2>

        {confirmSeries ? (
          <SeriesWarning
            onBack={() => setConfirmSeries(false)}
            onConfirm={() => {
              setConfirmSeries(false);
              save(true);
            }}
          />
        ) : deleteChoice ? (
          <DeleteChoice
            onBack={() => setDeleteChoice(false)}
            onDelete={(occurrence) => remove(occurrence)}
          />
        ) : (
          <div className="flex flex-col gap-3">
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Summary
              <input
                aria-label="Summary"
                value={summary}
                onChange={(event) => setSummary(event.target.value)}
                disabled={contentDisabled}
                className={fieldClass()}
              />
            </label>

            {!editing && (
              <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                Calendar
                <select
                  aria-label="Calendar"
                  value={calendarKey}
                  onChange={(event) => setCalendarKey(event.target.value)}
                  className={fieldClass()}
                >
                  {writable.map((calendar) => (
                    <option key={`${calendar.account}:${calendar.id}`} value={`${calendar.account}:${calendar.id}`}>
                      {calendar.name}
                    </option>
                  ))}
                </select>
              </label>
            )}

            {allDay ? (
              <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                Date
                <input
                  aria-label="Date"
                  type="date"
                  value={dateText}
                  onChange={(event) => setDateText(event.target.value)}
                  className={fieldClass()}
                />
              </label>
            ) : (
              <div className="flex gap-2">
                <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
                  Start
                  <input
                    aria-label="Start"
                    type="datetime-local"
                    value={startText}
                    onChange={(event) => setStartText(event.target.value)}
                    className={fieldClass()}
                  />
                </label>
                <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
                  End
                  <input
                    aria-label="End"
                    type="datetime-local"
                    value={endText}
                    onChange={(event) => setEndText(event.target.value)}
                    className={fieldClass()}
                  />
                </label>
              </div>
            )}

            {!editing && (
              <label className="flex items-center gap-2 text-xs text-muted-foreground">
                <input
                  aria-label="All day"
                  type="checkbox"
                  checked={allDay}
                  onChange={(event) => setAllDay(event.target.checked)}
                />
                All day
              </label>
            )}

            {editing && row.recurring && (
              <p className="text-xs text-muted-foreground">
                Repeats. The series holds its own rule.
              </p>
            )}

            {!editing && (
              <div className="flex gap-2">
                <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
                  Repeats
                  <select
                    aria-label="Repeats"
                    value={repeat}
                    onChange={(event) => setRepeat(event.target.value as Frequency | 'none')}
                    className={fieldClass()}
                  >
                    {REPEATS.map(({ id, label }) => (
                      <option key={id} value={id}>
                        {label}
                      </option>
                    ))}
                  </select>
                </label>
                {repeat !== 'none' && (
                  <label className="flex w-28 flex-col gap-1 text-xs text-muted-foreground">
                    Ends after
                    <input
                      aria-label="Ends after"
                      type="number"
                      min={1}
                      placeholder="never"
                      value={endsAfter}
                      onChange={(event) => setEndsAfter(event.target.value)}
                      className={fieldClass()}
                    />
                  </label>
                )}
              </div>
            )}

            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Location
              <input
                aria-label="Location"
                value={location}
                onChange={(event) => setLocation(event.target.value)}
                disabled={contentDisabled}
                placeholder={editing ? 'Leave empty to keep' : ''}
                className={fieldClass()}
              />
            </label>
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Notes
              <textarea
                aria-label="Notes"
                value={notes}
                onChange={(event) => setNotes(event.target.value)}
                disabled={contentDisabled}
                placeholder={editing ? 'Leave empty to keep' : ''}
                className="min-h-[56px] w-full rounded-[var(--radius-xs)] border border-input bg-background px-2 py-1 text-xs text-foreground outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring"
              />
            </label>

            {scopeable && (
              <fieldset className="flex gap-3 border-t border-border pt-2 text-xs text-foreground">
                <legend className="sr-only">Edit scope</legend>
                <label className="flex items-center gap-1.5">
                  <input
                    type="radio"
                    name="edit-scope"
                    aria-label="This event"
                    checked={scope === 'this'}
                    onChange={() => setScope('this')}
                  />
                  This event
                </label>
                <label className="flex items-center gap-1.5">
                  <input
                    type="radio"
                    name="edit-scope"
                    aria-label="All events"
                    checked={scope === 'all'}
                    onChange={() => setScope('all')}
                  />
                  All events
                </label>
              </fieldset>
            )}
            {contentDisabled && (
              <p className="text-xs text-muted-foreground">
                The title and notes belong to the whole series; the times are yours to move.
              </p>
            )}

            <div className="flex items-center justify-between gap-2 border-t border-border pt-3">
              {editing ? (
                <button
                  type="button"
                  onClick={() =>
                    row.recurring && row.occurrence_start !== undefined
                      ? setDeleteChoice(true)
                      : remove(false)
                  }
                  className="rounded-[var(--radius)] border border-[var(--destructive)] px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-hover"
                >
                  Delete
                </button>
              ) : (
                <span />
              )}
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={onClose}
                  className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={() => save()}
                  className="rounded-[var(--radius)] bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground transition-colors hover:opacity-90"
                >
                  Save
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
