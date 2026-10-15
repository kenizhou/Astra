// SPDX-License-Identifier: GPL-3.0-only
// The monogram foreground derivation: dark ink over light palette colours,
// white over dark ones, the muted ink when no colour stands.

import { describe, expect, it } from 'vitest';

import { monogramForeground } from './Avatar';

describe('the monogram foreground', () => {
  it('draws white ink on dark colours and dark ink on light colours', () => {
    expect(monogramForeground('#7a6ff0')).toBe('#ffffff');
    expect(monogramForeground('#10b981')).toBe('#ffffff');
    expect(monogramForeground('#f59e0b')).toBe('#1c1917');
    expect(monogramForeground('#fbbf24')).toBe('#1c1917');
  });

  it('falls back to the muted ink when no colour stands', () => {
    expect(monogramForeground(undefined)).toBe('var(--muted-foreground)');
    expect(monogramForeground('not-a-colour')).toBe('#ffffff');
  });
});
