// SPDX-License-Identifier: GPL-3.0-only
// The settings half of the IPC wire: the SettingsSnapshot mirror (the
// persisted settings block, the account rows with the masked dav echo,
// and the about block), the SettingsIntent family's constructors, and
// the snapshot pull. Split from ipc.ts to keep that file under the line
// cap, like the calendar and people halves before it, and re-exported
// whole through it, so ipc stays the one import point of the wire. The
// literals are pinned in settings.test.ts against the JSON astra-app's
// protocol_settings_tests.rs and astra-viewmodel's settings_tests.rs
// assert.

import { invoke } from '@tauri-apps/api/core';

import type { Signature } from './composerDocument';
import type { ViewMode } from './surfaces';

/** The theme the chrome renders; crosses as a snake_case word. */
export type ThemeMode = 'follow_host' | 'light' | 'dark';

/** A day of the week, in the engine's own weekday words. */
export type WeekdayWire = 'mo' | 'tu' | 'we' | 'th' | 'fr' | 'sa' | 'su';

/** The calendar new events land in: the account and the key together. */
export interface DefaultCalendar {
  account: string;
  key: string;
}

/** The persisted settings the screen renders, every field of it. */
export interface SettingsBlock {
  theme: ThemeMode;
  twenty_four_clock: boolean;
  zone: string;
  first_day_of_week: WeekdayWire;
  horizon_days: number;
  quote_below: boolean;
  view_mode: ViewMode;
  default_calendar?: DefaultCalendar;
  /** The account plain compositions open from; absent is unpinned. */
  default_send_account?: string;
  /** Whether plugins run at all (the admin policy switch); on by default. */
  plugins_enabled?: boolean;
  /** Whether new mail raises OS notifications; on by default. */
  notifications_enabled?: boolean;
}

/** One account's signature assignment: what its new mail and its replies
 *  open with. Either slot may stand absent while the other holds one. */
export interface AccountSignatures {
  new_mail?: Signature;
  reply?: Signature;
}

/**
 * How much of a message an account keeps a warm offline copy of; crosses
 * as a snake_case word, or a map for the byte figure.
 */
export type SyncBodyCap = 'off' | 'everything' | { bytes: { octets: number } };

/** One account's own sync settings, the effective values the row carries. */
export interface AccountSyncSettings {
  /** Months of history the account syncs; zero is the whole mailbox. */
  depth_months: number;
  /** The body cap state. */
  body_cap: SyncBodyCap;
  /** The polling cadence, in minutes. */
  poll_minutes: number;
}

/** An account's kind; crosses as a snake_case word. */
export type AccountKind = 'imap_smtp' | 'jmap' | 'dav' | 'microsoft' | 'google';

/** Which of an account's legs are standing. */
export interface AccountLegs {
  calendar: boolean;
  contacts: boolean;
}

/**
 * Why a Microsoft account's grant needs the person to act; crosses as a
 * snake_case word. A withheld calendar and a dead sign-in are different
 * prompts with the same remedy, a fresh sign-in.
 */
export type GrantStateWord = 'calendar_withheld' | 'sign_in_expired';

/**
 * The dav section an account's row echoes back: the editor's shape minus
 * the password. The credential never crosses the wire;
 * `dav_password_set` answers whether one is stored.
 */
export interface DavSectionRow {
  caldav_url?: string;
  carddav_url?: string;
  username: string;
  dav_password_set: boolean;
}

/** One configured account, as the accounts page renders it. */
export interface AccountSettingsRow {
  account: string;
  address: string;
  kind: AccountKind;
  legs: AccountLegs;
  dav?: DavSectionRow;
  dav_error?: string;
  refresh_error?: string;
  /**
   * The standing grant refusal the row carries, when the account's OAuth
   * grant needs the person to act. Absent is the healthy state.
   */
  grant?: GrantStateWord;
  /** The account's crypto method (none / openpgp / smime); the built-in send gate's switch. */
  crypto_method?: string;
  /** Why the account's crypto-method write last refused or failed, when it did. */
  crypto_error?: string;
  /** The account's signature assignment, when it holds one. */
  signatures?: AccountSignatures;
  /** The account's own sync settings, the effective values; absent is the product defaults. */
  sync?: AccountSyncSettings;
}

/** The About block: the version and the attributions. */
export interface AboutBlock {
  version: string;
  attributions?: string[];
}

/** Why one settings write refused or failed; a snake_case word. */
export type SettingsError =
  | 'persist_failed'
  | 'zone_unresolvable'
  | 'horizon_out_of_bounds'
  | 'remove_failed';

/** The settings snapshot the surface renders after every pull. */
export interface SettingsSnapshot {
  settings: SettingsBlock;
  accounts?: AccountSettingsRow[];
  about: AboutBlock;
  error?: SettingsError;
}

/**
 * The dav editor's shape: what the accounts page's form holds for one
 * account's section. Every field optional, because the editor states
 * only what the user typed: an edit of the username alone crosses no key
 * for the origins, and the password crosses only when a new one is
 * typed (the row's echo never carries it back).
 */
export interface DavSectionInput {
  caldav_url?: string;
  carddav_url?: string;
  username?: string;
  password?: string;
}

/**
 * What the settings surface asks the runtime to do, nested under the
 * `settings` tag of the intent that carries it. Built through the
 * `settingsIntents` constructors so the key order matches the Rust wire
 * byte for byte.
 */
export type SettingsIntentBody =
  | 'refresh'
  | { set_theme: { mode: ThemeMode } }
  | { set_clock: { twenty_four: boolean } }
  | { set_plugins_enabled: { enabled: boolean } }
  | { set_notifications_enabled: { enabled: boolean } }
  | { set_zone: { zone: string } }
  | { set_first_day_of_week: { weekday: WeekdayWire } }
  | { set_horizon: { days: number } }
  | { set_quote_style: { below: boolean } }
  | { set_view_mode: { mode: ViewMode } }
  | { set_default_calendar: { calendar: DefaultCalendar | null } }
  | { set_dav: { account: string; section: DavSectionInput | null } }
  | { remove_account: { account: string } }
  | { set_send_account: { account: string | null } }
  | { set_signature: { account: string; reply: boolean; signature: Signature | null } }
  | { set_crypto_method: { account: string; method: string } }
  | {
      set_sync_settings: {
        account: string;
        depth_months?: number;
        body_cap?: SyncBodyCap;
        poll_minutes?: number;
      };
    };

/**
 * The section constructor's omission discipline: a field the editor did
 * not state crosses no key at all, exactly as the Rust input's serde
 * skips do.
 */
function sectionOf(section: DavSectionInput) {
  return {
    ...(section.caldav_url !== undefined && { caldav_url: section.caldav_url }),
    ...(section.carddav_url !== undefined && { carddav_url: section.carddav_url }),
    ...(section.username !== undefined && { username: section.username }),
    ...(section.password !== undefined && { password: section.password }),
  };
}

/**
 * The settings constructors, spread into ipc's `intents`, the family in
 * the enum's own order.
 */
export const settingsIntents = {
  /** Rebuild and republish the settings snapshot. */
  refreshSettings: () => ({ settings: 'refresh' as const }),
  /** Set the theme: follow the host, or an explicit override. */
  setTheme: (mode: ThemeMode) => ({ settings: { set_theme: { mode } } }),
  /** Set the clock to 24-hour or 12-hour. */
  setClock: (twentyFour: boolean) => ({
    settings: { set_clock: { twenty_four: twentyFour } },
  }),
  /** Set whether plugins run at all (the admin policy switch). */
  setPluginsEnabled: (enabled: boolean) => ({
    settings: { set_plugins_enabled: { enabled } },
  }),
  /** Set whether new mail raises OS notifications. */
  setNotificationsEnabled: (enabled: boolean) => ({
    settings: { set_notifications_enabled: { enabled } },
  }),
  /** Set the calendar's display zone; the core refuses an id it cannot resolve. */
  setZone: (zone: string) => ({ settings: { set_zone: { zone } } }),
  /** Set the day the week starts on, in the engine's weekday words. */
  setFirstDayOfWeek: (weekday: WeekdayWire) => ({
    settings: { set_first_day_of_week: { weekday } },
  }),
  /** Set how many days the calendar's cold window covers. */
  setHorizon: (days: number) => ({ settings: { set_horizon: { days } } }),
  /** Set whether the composer quotes the original below the reply. */
  setQuoteStyle: (below: boolean) => ({ settings: { set_quote_style: { below } } }),
  /**
   * Set the mailbox list's grouping through the settings family's own
   * arm; the list intent's bare-word wire beside it stays untouched.
   */
  setSettingsViewMode: (mode: ViewMode) => ({ settings: { set_view_mode: { mode } } }),
  /** Pin the calendar new events land in, or unpin it with null. */
  setDefaultCalendar: (calendar: DefaultCalendar | null) => ({
    settings: { set_default_calendar: { calendar } },
  }),
  /** Write one account's dav section, or clear its legs with null. */
  setDav: (account: string, section: DavSectionInput | null) => ({
    settings: {
      set_dav: { account, section: section === null ? null : sectionOf(section) },
    },
  }),
  /** Remove an account: its stored config, its rows, and its legs go together. */
  removeAccount: (account: string) => ({ settings: { remove_account: { account } } }),
  /** Pin the account plain compositions open from, or unpin it with null. */
  setSendAccount: (account: string | null) => ({
    settings: { set_send_account: { account } },
  }),
  /** Write one slot of an account's signature assignment, or clear it with
   *  null; the last empty slot takes the account's row with it. */
  setSignature: (account: string, reply: boolean, signature: Signature | null) => ({
    settings: { set_signature: { account, reply, signature } },
  }),
  /** Write one account's crypto method (none / openpgp / smime): the
   *  built-in send gate's per-account switch. */
  setCryptoMethod: (account: string, method: string) => ({
    settings: { set_crypto_method: { account, method } },
  }),
  /**
   * Write one account's own sync settings; each field stated is written and
   * each omitted field crosses no key at all, exactly as the Rust arm's
   * serde skips do, so an edit of the depth alone leaves the cap and the
   * cadence standing.
   */
  setSyncSettings: (
    account: string,
    settings: { depth_months?: number; body_cap?: SyncBodyCap; poll_minutes?: number },
  ) => ({
    settings: {
      set_sync_settings: {
        account,
        ...(settings.depth_months !== undefined && { depth_months: settings.depth_months }),
        ...(settings.body_cap !== undefined && { body_cap: settings.body_cap }),
        ...(settings.poll_minutes !== undefined && { poll_minutes: settings.poll_minutes }),
      },
    },
  }),
};

/** Pulls the settings snapshot. */
export function settings(): Promise<SettingsSnapshot> {
  return invoke('settings');
}
