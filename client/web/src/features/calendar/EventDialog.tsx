// SPDX-License-Identifier: GPL-3.0-only
// The event dialog: creation over the anchor day, and the edit split the
// wire's own shapes force. A content edit crosses through the update intent;
// a time edit crosses through the drag currency, whose delta the core
// applies in the event's own wall clock: exact for floating and all-day
// events, the drag semantics for zoned ones. One-vs-all scopes a recurring
// row; a series edit that would discard per-occurrence changes is confirmed
// first; the repeat rule reads back in full over the detail read (the
// create select stays the editable vocabulary), because a rule half-seen
// is a rule silently rewritten, and the guest list rides the same
// read-only copy.

import { useMemo, useState } from 'react';

import { dispatch, intents } from '@/services/ipc';
import type {
  CalendarRef,
  EventDrag,
  Frequency,
  OccurrenceRow,
} from '@/services/ipc';

import { addDays, dayKeyOf } from './calendarRange';
import { localDelta, localSlotValue, localValueOf } from './localDateTime';
import { useCalendarStore } from './calendarStore';
import { DetailReadonlyCopy, useEventDetail } from './EventDetailRead';
import { useSettingsStore } from '@/features/settings/settingsStore';
import { useDialogFocus } from '@/components/ui/useDialogFocus';
import { DeleteChoice, SeriesWarning } from './EventDialogSteps';

/** The repeat choices a create offers. */
const REPEATS: ReadonlyArray<{ id: Frequency | 'none'; label: string }> = [
  { id: 'none', label: 'Does not repeat' },
  { id: 'daily', label: 'Daily' },
  { id: 'weekly', label: 'Weekly' },
  { id: 'monthly', label: 'Monthly' },
  { id: 'yearly', label: 'Yearly' },
];

/** The field chrome every input shares. */
function fieldClass(): string {
  return 'h-7 w-full rounded-full border border-input bg-background px-3 text-xs text-foreground outline-none transition-colors focus:border-primary';
}

interface EventDialogProps {
  /** The occurrence being edited, or null to create. */
  row: OccurrenceRow | null;
  /** The slot a drag-create claimed, prefilling a create's day and clocks. */
  slot?: { day: string; startMinutes: number; endMinutes: number } | null;
  /** Closes the dialog. */
  onClose: () => void;
}

/** The create and edit dialog over one event. */
export function EventDialog({ row, slot = null, onClose }: EventDialogProps) {
  const calendars = useCalendarStore((s) => s.calendars);
  const anchor = useCalendarStore((s) => s.anchor);
  const nowData = useCalendarStore((s) => s.nowData);
  const pinned = useSettingsStore((s) => s.settings.default_calendar);
  const zone = nowData?.zone ?? 'Etc/UTC';

  const writable = useMemo(
    () => calendars.filter((calendar) => calendar.can_write),
    [calendars],
  );
  // The settings block's pin decides where a create lands by default, the
  // diary's own flag standing in only while nothing is pinned or the pin
  // names a calendar that no longer exists.
  const defaultCalendar = useMemo(() => {
    if (pinned !== undefined) {
      const hit = writable.find(
        (calendar) => calendar.account === pinned.account && calendar.id === pinned.key,
      );
      if (hit !== undefined) {
        return hit;
      }
    }
    return writable.find((calendar) => calendar.is_default) ?? writable[0];
  }, [writable, pinned]);

  const editing = row !== null;
  const scopeable = editing && row.recurring && row.occurrence_start !== undefined;
  // The read-only copy the editor carries (the guest list and the rule
  // sentence): there so the person sees who they are writing to; an editor
  // changes neither this slice.
  const detail = useEventDetail(row);

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
  const day =
    row !== null ? dayKeyOf(new Date(row.start), zone) : (slot?.day ?? anchor ?? '2026-01-01');
  const [allDay, setAllDay] = useState(row?.all_day ?? false);
  const [startText, setStartText] = useState(
    row !== null
      ? localValueOf(row.start, zone)
      : slot !== null
        ? localSlotValue(slot.day, slot.startMinutes)
        : `${day}T09:00`,
  );
  const [endText, setEndText] = useState(
    row !== null
      ? localValueOf(row.end, zone)
      : slot !== null
        ? localSlotValue(slot.day, slot.endMinutes)
        : `${day}T10:00`,
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
    const edit: { title?: string; notes?: string; location?: string; occurrence?: string } = {};
    // An emptied summary is a title clear: the wire's three-state title
    // carries it rather than silently keeping the old words.
    if (summary.trim() !== row.title) {
      edit.title = summary.trim();
    }
    if (location.trim() !== '') {
      edit.location = location.trim();
    }
    if (notes.trim() !== '') {
      edit.notes = notes.trim();
    }
    // A content edit under This event lands on the one occurrence the wire
    // addresses by its token, the same scope the time edit takes; the token
    // rides an edit, never travels alone.
    if (Object.keys(edit).length > 0) {
      if (thisScope && row.occurrence_start !== undefined) {
        edit.occurrence = row.occurrence_start;
      }
      void dispatch(intents.updateCalendarEvent({ account: row.account, key: row.event }, edit));
    }
    const startChanged = startText !== localValueOf(row.start, zone);
    const endChanged = endText !== localValueOf(row.end, zone);
    if (allDay) {
      const moved = dateText !== day;
      if (moved && eventRef !== null) {
        const delta = localDelta(`${day}T00:00`, `${dateText}T00:00`);
        move('whole', delta.days, delta.minutes);
      }
    } else if ((startChanged || endChanged) && eventRef !== null) {
      // Both edges shift by the start's delta, the same whole-move the grid
      // drag makes; an end-only edit is the resize, the end's own delta on
      // the end edge, which the core clamps to the quarter-hour floor.
      if (startChanged) {
        const delta = localDelta(localValueOf(row.start, zone), startText);
        move('whole', delta.days, delta.minutes);
      } else {
        const delta = localDelta(localValueOf(row.end, zone), endText);
        move('end', delta.days, delta.minutes);
      }
    }
    onClose();
  };

  /** Sends the move intent, scoped to the one occurrence when it stands. */
  const move = (edge: 'whole' | 'end', days: number, minutes: number): void => {
    if (eventRef === null || row === null) {
      return;
    }
    const drag: EventDrag = {
      edge,
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

  // The dialog owns its dismissal: Escape closes it here and never bubbles
  // to the pane's clearSelection, and a click on the backdrop's own ground
  // closes it. The focus discipline rides the shared hook.
  const dialogRef = useDialogFocus<HTMLDivElement>();

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        ref={dialogRef}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            onClose();
          }
        }}
        className="flex max-h-full w-[420px] flex-col overflow-y-auto rounded-xl border border-border-subtle bg-card p-5 shadow-[var(--shadow-lg)] focus-visible:outline-none"
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

            {editing && detail !== null && (
              <DetailReadonlyCopy detail={detail} recurring={row.recurring} />
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
                placeholder={editing ? 'Leave empty to keep' : ''}
                className="min-h-[56px] w-full rounded-xl border border-input bg-background px-3 py-1 text-xs text-foreground outline-none transition-colors focus:border-primary"
              />
            </label>

            {scopeable && (
              <fieldset className="flex gap-3 border-t border-border-subtle pt-2 text-xs text-foreground">
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
            {!editing && writable.length === 0 && (
              <p className="text-xs text-[var(--warning)]">There is no calendar you can write to.</p>
            )}

            <div className="flex items-center justify-between gap-2 border-t border-border-subtle pt-3">
              {editing ? (
                <button
                  type="button"
                  onClick={() =>
                    row.recurring && row.occurrence_start !== undefined
                      ? setDeleteChoice(true)
                      : remove(false)
                  }
                  className="rounded-[var(--radius)] border border-[var(--destructive)] px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
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
                  className="rounded-[var(--radius)] border border-border-subtle px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={() => save()}
                  // With no writable calendar there is nothing to save into:
                  // the note says so rather than a save that silently no-ops.
                  disabled={!editing && writable.length === 0}
                  className="rounded-[var(--radius)] bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground shadow-[var(--shadow-sm)] transition-colors hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
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
