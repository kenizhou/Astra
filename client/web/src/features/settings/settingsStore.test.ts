// SPDX-License-Identifier: GPL-3.0-only
// The settings store: a pulled snapshot lands field for field, every apply
// moves the revision the editor watches, and the theme the block carries
// routes into the shell's own chrome state, so the persisted block is the
// one truth the whole shell renders.
import { describe, expect, it, beforeEach } from 'vitest';

import { useSettingsStore } from './settingsStore';
import { useUiStore } from '@/stores/uiStore';
import type { SettingsSnapshot } from '@/services/ipc';

/** A snapshot holding every block, overridable per test. */
function snapshot(over: Partial<SettingsSnapshot> = {}): SettingsSnapshot {
  return {
    settings: {
      theme: 'dark',
      twenty_four_clock: true,
      zone: 'Europe/Amsterdam',
      first_day_of_week: 'su',
      horizon_days: 31,
      quote_below: false,
      view_mode: 'threaded',
      default_calendar: { account: 'work', key: 'cal-1' },
    },
    accounts: [
      {
        account: 'work',
        address: 'ada@example.test',
        kind: 'imap_smtp',
        legs: { calendar: true, contacts: false },
      },
    ],
    about: { version: '0.1.0', attributions: ['email-calendar-sync-engine'] },
    ...over,
  };
}

beforeEach(() => {
  useSettingsStore.getState().applySnapshot(snapshot());
});

describe('the settings store apply', () => {
  it('lands the settings block, the account rows, the about block, and the error', () => {
    const state = useSettingsStore.getState();
    expect(state.settings).toEqual(snapshot().settings);
    expect(state.accounts).toEqual(snapshot().accounts);
    expect(state.about).toEqual(snapshot().about);
    expect(state.error).toBeUndefined();
  });

  it('keeps the optional blocks honest when the snapshot omits them', () => {
    useSettingsStore.getState().applySnapshot({
      settings: snapshot().settings,
      about: { version: '0.2.0' },
      error: 'persist_failed',
    });

    const state = useSettingsStore.getState();
    expect(state.accounts).toEqual([]);
    expect(state.about.attributions).toBeUndefined();
    expect(state.error).toBe('persist_failed');
  });

  it('moves the revision on every apply, so watches see a fresh publish', () => {
    const before = useSettingsStore.getState().revision;
    useSettingsStore.getState().applySnapshot(snapshot());
    expect(useSettingsStore.getState().revision).toBe(before + 1);
  });

  it('routes the block theme into the shell chrome', () => {
    expect(useUiStore.getState().themeMode).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    useSettingsStore.getState().applySnapshot(
      snapshot({ settings: { ...snapshot().settings, theme: 'follow_host' } }),
    );
    expect(useUiStore.getState().themeMode).toBe('follow_host');
  });
});
