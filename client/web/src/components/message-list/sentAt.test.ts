// SPDX-License-Identifier: GPL-3.0-only
// The list's sent-at stamp, pinned: the wire instant is UTC, and the list
// derives every label from its UTC parts so the stamp is stable wherever
// the app runs. The month and weekday words follow the language the
// settings block carries; the Today, Yesterday, and Earlier copy is the
// list's own and stays English until the copy catalogue grows.
import { act } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { useSettingsStore } from '@/features/settings/settingsStore';

import { formatSentAt, sentDayLabel } from './sentAt';

const NOON = new Date('2026-10-09T12:00:00Z');

function applyLanguage(language: string | undefined): void {
  act(() => {
    const settings = { ...useSettingsStore.getState().settings };
    if (language === undefined) {
      delete settings.language;
    } else {
      settings.language = language;
    }
    useSettingsStore.getState().applySnapshot({
      settings,
      accounts: [],
      about: { version: '0.1.0' },
    });
  });
}

describe('formatSentAt', () => {
  it('renders an unknown instant as an empty label', () => {
    expect(formatSentAt('', NOON)).toBe('');
  });

  it('renders a same-day instant as hours and minutes', () => {
    expect(formatSentAt('2026-10-09T08:30:00Z', NOON)).toBe('08:30');
    expect(formatSentAt('2026-10-09T16:05:00Z', NOON)).toBe('16:05');
    expect(formatSentAt('2026-10-09T00:00:00Z', NOON)).toBe('00:00');
  });

  it('renders an earlier day of the same year as day and month', () => {
    expect(formatSentAt('2026-10-08T16:05:00Z', NOON)).toBe('8 Oct');
    expect(formatSentAt('2026-01-02T09:00:00Z', NOON)).toBe('2 Jan');
  });

  it('renders an instant of another year with the year', () => {
    expect(formatSentAt('2025-12-31T23:59:00Z', NOON)).toBe('31 Dec 2025');
  });
});

describe('the clock setting', () => {
  it('spells the day label in the 12-hour form when the setting is off', () => {
    const stamp = '2026-10-12T12:10:00Z';
    const now = new Date('2026-10-12T15:00:00Z');
    expect(formatSentAt(stamp, now, true)).toBe('12:10');
    expect(formatSentAt(stamp, now, false)).toBe('12:10 PM');
    const evening = '2026-10-12T00:05:00Z';
    expect(formatSentAt(evening, now, false)).toBe('12:05 AM');
  });
});

describe('sentDayLabel', () => {
  it('groups an unknown instant under Earlier', () => {
    expect(sentDayLabel('', NOON)).toBe('Earlier');
    expect(sentDayLabel('not a date', NOON)).toBe('Earlier');
  });

  it('names the current and previous UTC days Today and Yesterday', () => {
    expect(sentDayLabel('2026-10-09T00:00:00Z', NOON)).toBe('Today');
    expect(sentDayLabel('2026-10-08T23:59:00Z', NOON)).toBe('Yesterday');
  });

  it('names the weekday within the last week', () => {
    // NOON is a Friday; the week behind it runs back to Saturday.
    expect(sentDayLabel('2026-10-07T09:00:00Z', NOON)).toBe('Wednesday');
    expect(sentDayLabel('2026-10-05T09:00:00Z', NOON)).toBe('Monday');
    expect(sentDayLabel('2026-10-04T09:00:00Z', NOON)).toBe('Sunday');
  });

  it('falls back to the date beyond the last week, with the year across years', () => {
    expect(sentDayLabel('2026-10-02T09:00:00Z', NOON)).toBe('2 Oct');
    expect(sentDayLabel('2025-12-31T23:59:00Z', NOON)).toBe('31 Dec 2025');
  });
});

describe('the language setting', () => {
  it('keeps the English month and weekday words for the default tag', () => {
    applyLanguage(undefined);
    expect(formatSentAt('2026-10-08T16:05:00Z', NOON)).toBe('8 Oct');
    expect(sentDayLabel('2026-10-07T09:00:00Z', NOON)).toBe('Wednesday');
  });

  it('spells the month and weekday words in the standing language', () => {
    applyLanguage('fr-FR');
    expect(formatSentAt('2026-10-08T16:05:00Z', NOON)).toBe('8 oct.');
    expect(sentDayLabel('2026-10-07T09:00:00Z', NOON)).toBe('mercredi');
    expect(formatSentAt('2025-12-31T23:59:00Z', NOON)).toBe('31 déc. 2025');
  });

  it('keeps the list copy English whatever the tag stands at', () => {
    applyLanguage('fr-FR');
    expect(sentDayLabel('2026-10-09T00:00:00Z', NOON)).toBe('Today');
    expect(sentDayLabel('2026-10-08T23:59:00Z', NOON)).toBe('Yesterday');
    expect(sentDayLabel('', NOON)).toBe('Earlier');
  });
});
