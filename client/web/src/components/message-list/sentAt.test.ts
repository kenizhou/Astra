// SPDX-License-Identifier: GPL-3.0-only
// The list's sent-at stamp, pinned: the wire instant is UTC, and the list
// derives every label from its UTC parts so the stamp is stable wherever
// the app runs.
import { describe, expect, it } from 'vitest';

import { formatSentAt } from './sentAt';

const NOON = new Date('2026-10-09T12:00:00Z');

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
