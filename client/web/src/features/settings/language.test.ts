// SPDX-License-Identifier: GPL-3.0-only
// The language preference's surface half: the standing tag the snapshot
// carries, the default when the block names none, and the picker's offered
// set, which is the mechanism's own honesty (one catalogue today).
import { act } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { useSettingsStore } from './settingsStore';

import { currentLanguage, DEFAULT_LANGUAGE, LANGUAGES } from './language';

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

describe('the language preference', () => {
  it('answers the default tag when the block names none', () => {
    applyLanguage(undefined);
    expect(currentLanguage()).toBe(DEFAULT_LANGUAGE);
  });

  it('answers the standing tag the snapshot carries', () => {
    applyLanguage('fr-FR');
    expect(currentLanguage()).toBe('fr-FR');
  });

  it('offers exactly the one catalogue that ships today', () => {
    expect(LANGUAGES).toEqual([{ id: 'en-GB', label: 'English (United Kingdom)' }]);
  });
});
