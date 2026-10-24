// SPDX-License-Identifier: GPL-3.0-only
// The day key labels follow the language preference: the same UTC-midnight
// identity, the same zone discipline, the catalogue's own words. The
// calendar's display zone is untouched by the tag: the labels still read
// over UTC instants whatever the language stands at.
import { act } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { useSettingsStore } from '@/features/settings/settingsStore';

import {
  agendaDayLabel,
  dayMonthLabel,
  fullDayLabel,
  monthYearLabel,
  weekdayShortLabel,
} from './dayLabels';

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

describe('the day labels', () => {
  it('spell the English forms for the default tag', () => {
    applyLanguage(undefined);
    expect(fullDayLabel('2026-10-19')).toBe('Monday 19 October 2026');
    expect(dayMonthLabel('2026-10-19')).toBe('19 October');
    expect(agendaDayLabel('2026-10-19')).toBe('Monday 19 October');
    expect(monthYearLabel('2026-10-19')).toBe('October 2026');
    expect(weekdayShortLabel('2026-10-19')).toBe('Mon');
  });

  it('follow the language the settings block carries', () => {
    applyLanguage('fr-FR');
    expect(fullDayLabel('2026-10-19')).toBe('lundi 19 octobre 2026');
    expect(monthYearLabel('2026-10-19')).toBe('octobre 2026');
    expect(weekdayShortLabel('2026-10-19')).toBe('lun.');
  });

  it('keep the UTC zone whatever the language stands at', () => {
    applyLanguage('fr-FR');
    // The label derives from the key's UTC-midnight identity alone; the
    // en-GB fixture's comma normalisation holds across catalogues.
    expect(fullDayLabel('2026-01-01')).toBe('jeudi 1 janvier 2026');
  });
});
