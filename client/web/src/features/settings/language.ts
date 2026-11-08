// SPDX-License-Identifier: GPL-3.0-only
// The language preference's surface half: the standing tag the settings
// snapshot carries, held here for every formatter and localised surface to
// read, and the picker's offered set, which is the mechanism's own
// honesty. The tag carries the mechanism, not a catalogue of
// translations: the offered list names exactly the catalogues that ship,
// and the core holds no language list of its own. The store's snapshot
// apply is the one writer; the formatters read, so a mocked store in a
// test never breaks the labels.

/** The tag every surface reads before the preference names another. */
export const DEFAULT_LANGUAGE = 'en-GB';

/** The offered set: one entry per catalogue that ships, no more. */
export const LANGUAGES: ReadonlyArray<{ id: string; label: string }> = [
  { id: 'en-GB', label: 'English (United Kingdom)' },
];

/** The standing tag; the settings store's apply is the one writer. */
let standing: string = DEFAULT_LANGUAGE;

/** Takes the snapshot's language field; absent or empty keeps the default. */
export function setStandingLanguage(language: string | undefined): void {
  standing = language === undefined || language === '' ? DEFAULT_LANGUAGE : language;
}

/** The standing language tag. */
export function currentLanguage(): string {
  return standing;
}

/** One month's short name in the standing language; `month` is 0 for
 *  January, and the word derives from the locale's own catalogue over a
 *  UTC instant, so it never shifts with the host's clock. */
export function shortMonthName(month: number): string {
  return new Intl.DateTimeFormat(standing, { month: 'short', timeZone: 'UTC' }).format(
    new Date(Date.UTC(2026, month, 1)),
  );
}

/** One weekday's long name in the standing language; `day` is 0 for
 *  Sunday, over the same UTC derivation. */
export function weekdayName(day: number): string {
  return new Intl.DateTimeFormat(standing, { weekday: 'long', timeZone: 'UTC' }).format(
    new Date(Date.UTC(2026, 10, 1 + day)),
  );
}
