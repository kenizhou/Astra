// SPDX-License-Identifier: GPL-3.0-only
// The first localised set: the Diagnostics, Privacy, and About copy keyed
// by the language tag, English the only catalogue today, an unknown tag
// falling back to it. The mechanism is the deliverable, not translations,
// so the pins hold the honest wording the surfaces owe.
import { describe, expect, it } from 'vitest';

import { ENGLISH, stringsFor } from './strings';

describe('the strings catalogue', () => {
  it('answers the English catalogue for its own tag', () => {
    expect(stringsFor('en-GB')).toBe(ENGLISH);
  });

  it('falls back to English for a tag with no catalogue', () => {
    expect(stringsFor('fr-FR')).toBe(ENGLISH);
    expect(stringsFor('')).toBe(ENGLISH);
  });

  it('names what the log never carries in the Diagnostics privacy note', () => {
    const note = stringsFor('en-GB').diagnostics.privacyNote;
    expect(note).toContain('no message content');
    expect(note).toContain('no credentials');
  });

  it('carries the next-launch wording for the debug toggle', () => {
    expect(stringsFor('en-GB').diagnostics.debugNote).toBe(
      'takes effect the next time Astra Mail starts',
    );
  });

  it('carries the plain Privacy statement and the plugin registry link', () => {
    const privacy = stringsFor('en-GB').privacy;
    expect(privacy.staysOnDevice.length).toBeGreaterThan(0);
    expect(privacy.aiSends).toContain('model endpoint');
    expect(privacy.pluginSentence).toContain('permissions you grant');
    expect(privacy.pluginLink).toBe('Open the plugin registry');
  });

  it('carries the About copy', () => {
    const about = stringsFor('en-GB').about;
    expect(about.appName).toBe('Astra Mail');
    expect(about.versionLabel).toBe('Version');
    expect(about.attributionsLabel).toBe('Attributions');
  });
});
