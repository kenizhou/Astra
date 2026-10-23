// SPDX-License-Identifier: GPL-3.0-only
// The settings wire, pinned against the literal JSON astra-app's
// protocol_settings_tests.rs and astra-viewmodel's settings_tests.rs
// assert: every intent crosses as exactly the bytes the Rust side reads,
// the shared fixture parses as the snapshot on both sides of the wire,
// and the pull asks by its command name.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import type { AccountSettingsRow, SettingsSnapshot } from './settings';
import { settingsIntents } from './settings';
import fixtureJson from './__fixtures__/settings_snapshot.json';

// The shared fixture, byte-for-byte the JSON astra-viewmodel's
// settings_tests.rs pins for a snapshot holding every block (the Rust
// suite reads this same file through its own include).
describe('the settings snapshot wire', () => {
  it('carries the settings block, the masked account row, and the about block', () => {
    const snapshot = fixtureJson as SettingsSnapshot;

    expect(snapshot.settings).toEqual({
      theme: 'dark',
      twenty_four_clock: true,
      zone: 'Europe/Amsterdam',
      first_day_of_week: 'su',
      horizon_days: 31,
      quote_below: false,
      view_mode: 'threaded',
      plugins_enabled: true,
      default_calendar: { account: 'work', key: 'cal-1' },
    });
    expect(snapshot.accounts).toEqual([
      {
        account: 'work',
        address: 'ada@example.test',
        kind: 'imap_smtp',
        legs: { calendar: true, contacts: false },
        dav: {
          caldav_url: 'https://dav.example.test',
          username: 'ada@example.test',
          dav_password_set: true,
        },
        dav_error: 'the re-dial could not reach the origin',
        refresh_error:
          'the diary could not be re-expanded under the new zone; set the zone again to retry',
        crypto_method: 'none',
        sync: {
          depth_months: 12,
          body_cap: { bytes: { octets: 1024 } },
          poll_minutes: 90,
        },
      },
    ]);
    expect(snapshot.about).toEqual({
      version: '0.1.0',
      attributions: ['email-calendar-sync-engine', 'pi', 'web'],
    });
    expect(snapshot.error).toBe('persist_failed');
  });

  it('never carries a password field on the masked dav row', () => {
    const snapshot = fixtureJson as SettingsSnapshot;
    const dav = snapshot.accounts?.[0]?.dav;
    expect(dav).toBeDefined();
    expect(Object.keys(dav!)).not.toContain('password');
    expect(dav!.dav_password_set).toBe(true);
  });

  it('carries the grant refusal words and no key for a healthy account', () => {
    // The words the refusal surfaces branch on, pinned against the JSON
    // astra-viewmodel's settings tests assert.
    const withheld = JSON.parse(
      '{"account":"work","address":"a@e.test","kind":"microsoft","legs":{"calendar":false,"contacts":false},"grant":"calendar_withheld"}',
    ) as AccountSettingsRow;
    expect(withheld.grant).toBe('calendar_withheld');
    const expired = JSON.parse(
      '{"account":"work","address":"a@e.test","kind":"microsoft","legs":{"calendar":false,"contacts":false},"grant":"sign_in_expired"}',
    ) as AccountSettingsRow;
    expect(expired.grant).toBe('sign_in_expired');
    const google = JSON.parse(
      '{"account":"work","address":"a@gmail.example","kind":"google","legs":{"calendar":true,"contacts":false},"grant":"sign_in_expired"}',
    ) as AccountSettingsRow;
    expect(google.kind).toBe('google');
    expect(google.grant).toBe('sign_in_expired');
    const healthy = JSON.parse(
      '{"account":"work","address":"a@e.test","kind":"microsoft","legs":{"calendar":false,"contacts":false}}',
    ) as AccountSettingsRow;
    expect(healthy.grant).toBeUndefined();
  });

  it('crosses an at-rest snapshot with no accounts and no error key', () => {
    const snapshot = JSON.parse(
      '{"settings":{"theme":"follow_host","twenty_four_clock":false,"zone":"","first_day_of_week":"mo","horizon_days":0,"quote_below":false,"view_mode":"flat","plugins_enabled":true},"about":{"version":""}}',
    ) as SettingsSnapshot;
    expect(snapshot.accounts).toBeUndefined();
    expect(snapshot.error).toBeUndefined();
    expect(snapshot.settings.default_calendar).toBeUndefined();
    expect(snapshot.settings.default_send_account).toBeUndefined();
    expect(snapshot.about.attributions).toBeUndefined();
  });

  it('carries the default send account and the signature assignment additively', () => {
    const snapshot = JSON.parse(
      '{"settings":{"theme":"follow_host","twenty_four_clock":false,"zone":"","first_day_of_week":"mo","horizon_days":42,"quote_below":true,"view_mode":"flat","default_send_account":"work"},"accounts":[{"account":"work","address":"ada@work.test","kind":"imap_smtp","legs":{"calendar":false,"contacts":false},"signatures":{"new_mail":{"body_html":"<p>Ada</p>","body_plain":"Ada"}}}],"about":{"version":""}}',
    ) as SettingsSnapshot;
    expect(snapshot.settings.default_send_account).toBe('work');
    expect(snapshot.accounts?.[0]?.signatures?.new_mail).toEqual({
      body_html: '<p>Ada</p>',
      body_plain: 'Ada',
    });
    expect(snapshot.accounts?.[0]?.signatures?.reply).toBeUndefined();
  });

  it('pulls the snapshot by its command name', async () => {
    invoke.mockResolvedValue({});
    const { settings } = await import('./settings');
    await settings();
    expect(invoke).toHaveBeenCalledWith('settings');
  });
});

// Every literal is byte-for-byte the JSON astra-app's
// protocol_settings_tests.rs asserts, so neither side of the wire can
// drift.
describe('the settings intent wire', () => {
  it('crosses refresh as its bare word', () => {
    expect(JSON.stringify(settingsIntents.refreshSettings())).toBe('{"settings":"refresh"}');
  });

  it('carries the theme, the clock, and the quote style', () => {
    expect(JSON.stringify(settingsIntents.setTheme('follow_host'))).toBe(
      '{"settings":{"set_theme":{"mode":"follow_host"}}}',
    );
    expect(JSON.stringify(settingsIntents.setTheme('light'))).toBe(
      '{"settings":{"set_theme":{"mode":"light"}}}',
    );
    expect(JSON.stringify(settingsIntents.setTheme('dark'))).toBe(
      '{"settings":{"set_theme":{"mode":"dark"}}}',
    );
    expect(JSON.stringify(settingsIntents.setClock(true))).toBe(
      '{"settings":{"set_clock":{"twenty_four":true}}}',
    );
    expect(JSON.stringify(settingsIntents.setQuoteStyle(false))).toBe(
      '{"settings":{"set_quote_style":{"below":false}}}',
    );
  });

  it('carries the zone verbatim and the weekday in the engine words', () => {
    expect(JSON.stringify(settingsIntents.setZone('Europe/Amsterdam'))).toBe(
      '{"settings":{"set_zone":{"zone":"Europe/Amsterdam"}}}',
    );
    expect(JSON.stringify(settingsIntents.setFirstDayOfWeek('su'))).toBe(
      '{"settings":{"set_first_day_of_week":{"weekday":"su"}}}',
    );
    expect(JSON.stringify(settingsIntents.setFirstDayOfWeek('mo'))).toBe(
      '{"settings":{"set_first_day_of_week":{"weekday":"mo"}}}',
    );
  });

  it('carries the horizon and the settings family view mode', () => {
    expect(JSON.stringify(settingsIntents.setHorizon(42))).toBe(
      '{"settings":{"set_horizon":{"days":42}}}',
    );
    expect(JSON.stringify(settingsIntents.setSettingsViewMode('threaded'))).toBe(
      '{"settings":{"set_view_mode":{"mode":"threaded"}}}',
    );
    expect(JSON.stringify(settingsIntents.setSettingsViewMode('flat'))).toBe(
      '{"settings":{"set_view_mode":{"mode":"flat"}}}',
    );
  });

  it('names the default calendar or unpins it with an explicit null', () => {
    expect(
      JSON.stringify(settingsIntents.setDefaultCalendar({ account: 'work', key: 'cal-1' })),
    ).toBe('{"settings":{"set_default_calendar":{"calendar":{"account":"work","key":"cal-1"}}}}');
    expect(JSON.stringify(settingsIntents.setDefaultCalendar(null))).toBe(
      '{"settings":{"set_default_calendar":{"calendar":null}}}',
    );
  });

  it('carries the editor section, skipping what is absent, or clears it with null', () => {
    expect(
      JSON.stringify(
        settingsIntents.setDav('work', {
          caldav_url: 'https://dav.example.test',
          carddav_url: 'https://cards.example.test',
          username: 'ada@example.test',
          password: 'app-password',
        }),
      ),
    ).toBe(
      '{"settings":{"set_dav":{"account":"work","section":{"caldav_url":"https://dav.example.test","carddav_url":"https://cards.example.test","username":"ada@example.test","password":"app-password"}}}}',
    );
    expect(
      JSON.stringify(settingsIntents.setDav('work', { username: 'ada@example.test' })),
    ).toBe('{"settings":{"set_dav":{"account":"work","section":{"username":"ada@example.test"}}}}');
    expect(JSON.stringify(settingsIntents.setDav('work', null))).toBe(
      '{"settings":{"set_dav":{"account":"work","section":null}}}',
    );
  });

  it('names the account a removal takes', () => {
    expect(JSON.stringify(settingsIntents.removeAccount('work'))).toBe(
      '{"settings":{"remove_account":{"account":"work"}}}',
    );
  });

  it('names the default send account or unpins it with an explicit null', () => {
    expect(JSON.stringify(settingsIntents.setSendAccount('home'))).toBe(
      '{"settings":{"set_send_account":{"account":"home"}}}',
    );
    expect(JSON.stringify(settingsIntents.setSendAccount(null))).toBe(
      '{"settings":{"set_send_account":{"account":null}}}',
    );
  });

  it('writes one signature slot with both legs, or clears it with null', () => {
    expect(
      JSON.stringify(
        settingsIntents.setSignature('work', false, { body_html: '<p>Ada</p>', body_plain: 'Ada' }),
      ),
    ).toBe(
      '{"settings":{"set_signature":{"account":"work","reply":false,"signature":{"body_html":"<p>Ada</p>","body_plain":"Ada"}}}}',
    );
    expect(JSON.stringify(settingsIntents.setSignature('work', true, null))).toBe(
      '{"settings":{"set_signature":{"account":"work","reply":true,"signature":null}}}',
    );
  });
});

// The dispatch ride: the constructors spread into ipc's `intents` under
// the settings tag, one object per verb.
describe('the settings command wire', () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it('spreads the family under the settings tag of the dispatch intent', async () => {
    const { intents } = await import('./ipc');
    expect(JSON.stringify(intents.refreshSettings())).toBe('{"settings":"refresh"}');
    expect(JSON.stringify(intents.setTheme('dark'))).toBe(
      '{"settings":{"set_theme":{"mode":"dark"}}}',
    );
    expect(JSON.stringify(intents.removeAccount('work'))).toBe(
      '{"settings":{"remove_account":{"account":"work"}}}',
    );
  });
});

// The per-account sync settings: the row carries the effective triple, and
// the intent crosses only the fields the editor named, exactly as the Rust
// arm's serde skips spell them.
describe('the per-account sync settings wire', () => {
  it('carries the effective triple on the account row', () => {
    const snapshot = fixtureJson as SettingsSnapshot;
    const row = snapshot.accounts?.[0];
    expect(row?.sync).toEqual({
      depth_months: 12,
      body_cap: { bytes: { octets: 1024 } },
      poll_minutes: 90,
    });
  });

  it('crosses only the named fields of a sync settings write', () => {
    expect(
      JSON.stringify(settingsIntents.setSyncSettings('work', { depth_months: 12 })),
    ).toBe('{"settings":{"set_sync_settings":{"account":"work","depth_months":12}}}');
    expect(
      JSON.stringify(
        settingsIntents.setSyncSettings('work', { body_cap: 'off', poll_minutes: 90 }),
      ),
    ).toBe(
      '{"settings":{"set_sync_settings":{"account":"work","body_cap":"off","poll_minutes":90}}}',
    );
  });
});
