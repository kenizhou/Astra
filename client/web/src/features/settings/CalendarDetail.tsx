// SPDX-License-Identifier: GPL-3.0-only
// The Calendar detail: the display zone picker over the browser's own IANA
// list (the app refuses typedly an id it cannot resolve, and the field
// keeps its prior value when it does), the first-day picker, the horizon
// picker whose typed draft commits on blur or Enter inside the core's own
// 7 to 365 (a refused commit keeps the prior value and says so; the core's
// own refusal says the same), and the default calendar over the writable
// calendars the diary holds, pinned or unpinned.

import { useEffect, useMemo, useState } from 'react';

import { dispatch, intents } from '@/services/ipc';
import type { WeekdayWire } from '@/services/ipc';
import { useCalendarStore } from '@/features/calendar/calendarStore';

import { PERSIST_SENTENCE } from './GeneralDetail';
import { useSettingsStore } from './settingsStore';

/** The core's own horizon bound: fewer or more days never crosses. */
const HORIZON_MIN_DAYS = 7;
const HORIZON_MAX_DAYS = 365;

/** The sentence an out-of-bounds horizon draws, typed here or refused there. */
const HORIZON_SENTENCE = 'The horizon stands between 7 and 365 days.';

/** The zone choices: the browser's own list, the persisted zone kept on it. */
function zoneOptions(zone: string): string[] {
  // Not every engine ships the list; without it the persisted zone stands
  // alone rather than a list the browser never vouched for.
  const intl = Intl as { supportedValuesOf?: (key: string) => string[] };
  const listed = intl.supportedValuesOf?.('timeZone') ?? [];
  return listed.includes(zone) ? listed : [zone, ...listed];
}

/** The weekday choices, Sunday first: the order a human reads. */
const WEEKDAYS: ReadonlyArray<{ id: WeekdayWire; label: string }> = [
  { id: 'su', label: 'Sunday' },
  { id: 'mo', label: 'Monday' },
  { id: 'tu', label: 'Tuesday' },
  { id: 'we', label: 'Wednesday' },
  { id: 'th', label: 'Thursday' },
  { id: 'fr', label: 'Friday' },
  { id: 'sa', label: 'Saturday' },
];

/** The select chrome every picker in the detail shares. */
const fieldClass =
  'h-7 w-full rounded-[var(--radius-xs)] border border-input bg-background px-2 text-xs text-foreground outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring';

/** The Calendar page of the settings screen. */
export function CalendarDetail() {
  const settings = useSettingsStore((s) => s.settings);
  const error = useSettingsStore((s) => s.error);
  const calendars = useCalendarStore((s) => s.calendars);

  const horizon = String(settings.horizon_days);
  const [horizonDraft, setHorizonDraft] = useState(horizon);
  const [horizonRefused, setHorizonRefused] = useState(false);
  // The draft follows the persisted block when the block moves elsewhere;
  // between a dispatch and its republish the typed value stands.
  useEffect(() => {
    setHorizonDraft(horizon);
  }, [horizon]);
  // The bound answers at the commit, never per keystroke: a controlled
  // input that refuses each keystroke restores its prop mid-word and
  // leaves most horizons untypeable.
  const commitHorizon = (): void => {
    const days = Number(horizonDraft);
    if (Number.isInteger(days) && days >= HORIZON_MIN_DAYS && days <= HORIZON_MAX_DAYS) {
      setHorizonRefused(false);
      if (days !== settings.horizon_days) {
        void dispatch(intents.setHorizon(days));
      }
      return;
    }
    setHorizonRefused(true);
    setHorizonDraft(horizon);
  };

  const writable = useMemo(
    () => calendars.filter((calendar) => calendar.can_write),
    [calendars],
  );
  const pinned = settings.default_calendar;
  const pinnedKey =
    pinned !== undefined &&
    writable.some((calendar) => calendar.account === pinned.account && calendar.id === pinned.key)
      ? `${pinned.account}:${pinned.key}`
      : '';
  const zones = useMemo(() => zoneOptions(settings.zone), [settings.zone]);

  return (
    <section aria-label="Calendar settings" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">Calendar</h2>
      <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
        Time zone
        <select
          aria-label="Time zone"
          value={settings.zone}
          onChange={(event) => void dispatch(intents.setZone(event.target.value))}
          className={fieldClass}
        >
          {zones.map((zone) => (
            <option key={zone} value={zone}>
              {zone}
            </option>
          ))}
        </select>
      </label>
      {error === 'zone_unresolvable' && (
        <p role="alert" className="max-w-[280px] text-xs text-[var(--warning)]">
          That time zone is not known on this device. Pick one from the list.
        </p>
      )}
      <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
        First day of week
        <select
          aria-label="First day of week"
          value={settings.first_day_of_week}
          onChange={(event) =>
            void dispatch(intents.setFirstDayOfWeek(event.target.value as WeekdayWire))
          }
          className={fieldClass}
        >
          {WEEKDAYS.map(({ id, label }) => (
            <option key={id} value={id}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
        Horizon days
        <input
          aria-label="Horizon days"
          type="number"
          min={HORIZON_MIN_DAYS}
          max={HORIZON_MAX_DAYS}
          value={horizonDraft}
          onChange={(event) => {
            setHorizonDraft(event.target.value);
            setHorizonRefused(false);
          }}
          onBlur={commitHorizon}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              commitHorizon();
            }
          }}
          className={fieldClass}
        />
      </label>
      {(horizonRefused || error === 'horizon_out_of_bounds') && (
        <p role="alert" className="max-w-[280px] text-xs text-[var(--warning)]">
          {HORIZON_SENTENCE}
        </p>
      )}
      <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
        Default calendar
        <select
          aria-label="Default calendar"
          value={pinnedKey}
          onChange={(event) => {
            const value = event.target.value;
            if (value === '') {
              void dispatch(intents.setDefaultCalendar(null));
              return;
            }
            const [account, key] = value.split(':');
            if (account !== undefined && key !== undefined) {
              void dispatch(intents.setDefaultCalendar({ account, key }));
            }
          }}
          className={fieldClass}
        >
          <option value="">None</option>
          {writable.map((calendar) => (
            <option key={`${calendar.account}:${calendar.id}`} value={`${calendar.account}:${calendar.id}`}>
              {calendar.name}
            </option>
          ))}
        </select>
      </label>
      {error === 'persist_failed' && (
        <p role="alert" className="text-xs text-[var(--warning)]">
          {PERSIST_SENTENCE}
        </p>
      )}
    </section>
  );
}
