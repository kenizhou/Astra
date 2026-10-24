// SPDX-License-Identifier: GPL-3.0-only
// The first localised set: the Diagnostics, Privacy, and About copy
// through one catalogue keyed by the language tag. English is the only
// catalogue today; the mechanism is the deliverable, not translations, so
// an unknown tag falls back to English rather than guessing, and a new
// catalogue lands as one more entry beside the keyed lookup.

import { currentLanguage } from './language';

/** The Diagnostics category's copy. */
export interface DiagnosticsStrings {
  title: string;
  readButton: string;
  tailLabel: string;
  copyButton: string;
  copiedNote: string;
  copyRefusedNote: string;
  exportButton: string;
  exportedTo: (path: string) => string;
  exportFailed: (reason: string) => string;
  debugToggle: string;
  debugNote: string;
  privacyNote: string;
}

/** The Privacy category's copy. */
export interface PrivacyStrings {
  title: string;
  staysOnDevice: string;
  aiSends: string;
  pluginSentence: string;
  analyticsToggle: string;
  analyticsNote: string;
  pluginLink: string;
}

/** The About category's copy. */
export interface AboutStrings {
  title: string;
  appName: string;
  versionLabel: string;
  attributionsLabel: string;
}

/** One language's whole first localised set. */
export interface SettingsStrings {
  diagnostics: DiagnosticsStrings;
  privacy: PrivacyStrings;
  about: AboutStrings;
}

/** The only catalogue that ships today. */
export const ENGLISH: SettingsStrings = {
  diagnostics: {
    title: 'Diagnostics',
    readButton: 'Read the log',
    tailLabel: 'Log tail',
    copyButton: 'Copy for support',
    copiedNote: 'Copied. Paste it into your support reply.',
    copyRefusedNote: 'The clipboard refused. Select the tail and copy it yourself.',
    exportButton: 'Export the log',
    exportedTo: (path) => `Exported to ${path}`,
    exportFailed: (reason) => reason,
    debugToggle: 'Debug logging',
    debugNote: 'takes effect the next time Astra Mail starts',
    privacyNote:
      'The log carries no message content, no credentials, and no file paths.',
  },
  privacy: {
    title: 'Privacy',
    staysOnDevice:
      'Your mail, contacts, and calendar stay on this device: the stores and the ledgers never leave it, the ledgers record counts and shapes only, and the log carries no message content, no credentials, and no file paths.',
    aiSends:
      'The AI features send what a prompt needs to the model endpoint you configured: the sender, the subject, and a capped slice of the body, never your credentials, and only after the data-loss policy you set.',
    pluginSentence:
      "Plugins run only with the permissions you grant them; each one's grants are listed in the plugin registry.",
    analyticsToggle: 'Product analytics',
    analyticsNote:
      'Off by default. Consent and withdrawal are the same switch: one click turns recording off, nothing is recorded while it is off, and the recorded events are counts and shapes kept on this device.',
    pluginLink: 'Open the plugin registry',
  },
  about: {
    title: 'About',
    appName: 'Astra Mail',
    versionLabel: 'Version',
    attributionsLabel: 'Attributions',
  },
};

/** One entry per catalogue that ships, keyed by its tag. */
const CATALOGUES: Record<string, SettingsStrings> = {
  'en-GB': ENGLISH,
};

/**
 * One tag's catalogue: the entry that ships for it, English for a tag
 * with none, so the mechanism never guesses a half translation.
 */
export function stringsFor(language: string): SettingsStrings {
  return CATALOGUES[language] ?? ENGLISH;
}

/** The standing language's catalogue, as the surfaces render it. */
export function currentStrings(): SettingsStrings {
  return stringsFor(currentLanguage());
}
