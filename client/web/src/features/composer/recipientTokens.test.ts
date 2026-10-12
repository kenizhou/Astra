// SPDX-License-Identifier: GPL-3.0-only
// The token math: the trailing token after the last comma, the finished
// recipients before it (or every part of a settled field), the bare insert
// with its separator for the next recipient, the rebuild of the string from
// the pills and the token, the per-recipient removal, and the seed
// normalisation.
import { describe, expect, it } from 'vitest';

import {
  finishedRecipients,
  insertBareAddress,
  normaliseRecipients,
  rebuildRecipients,
  removeRecipient,
  trailingToken,
} from './recipientTokens';

describe('the token math', () => {
  it('takes the token from after the last comma, verbatim', () => {
    expect(trailingToken('bo')).toBe('bo');
    expect(trailingToken('a@x.com, bo')).toBe(' bo');
    expect(trailingToken('a@x.com,bo')).toBe('bo');
    expect(trailingToken('')).toBe('');
  });

  it('splits the finished recipients before the token, blanks dropped', () => {
    expect(finishedRecipients('a@x.com, bo', false)).toEqual(['a@x.com']);
    expect(finishedRecipients('a@x.com, , bo', false)).toEqual(['a@x.com']);
    expect(finishedRecipients('bo', false)).toEqual([]);
    expect(finishedRecipients('', false)).toEqual([]);
  });

  it('treats every part of a settled field as finished', () => {
    expect(finishedRecipients('a@x.com, b@y.com', true)).toEqual(['a@x.com', 'b@y.com']);
    expect(finishedRecipients('a@x.com', true)).toEqual(['a@x.com']);
    expect(finishedRecipients('  , a@x.com , ', true)).toEqual(['a@x.com']);
  });

  it('inserts a bare address in place of the token, with the separator for the next', () => {
    expect(insertBareAddress('bo', 'bob@y.com')).toBe('bob@y.com, ');
    expect(insertBareAddress('a@x.com, bo', 'bob@y.com')).toBe('a@x.com, bob@y.com, ');
    expect(insertBareAddress('a@x.com,  al', 'alicia@y.com')).toBe('a@x.com, alicia@y.com, ');
  });

  it('rebuilds the string from the pills and the token', () => {
    expect(rebuildRecipients(['a@x.com'], ' bo')).toBe('a@x.com, bo');
    expect(rebuildRecipients(['a@x.com', 'b@y.com'], '')).toBe('a@x.com, b@y.com');
    expect(rebuildRecipients(['a@x.com'], 'bo')).toBe('a@x.com,bo');
    expect(rebuildRecipients([], 'bo')).toBe('bo');
    expect(rebuildRecipients([], '')).toBe('');

    const field = 'a@x.com, b@y.com, bo';
    expect(rebuildRecipients(finishedRecipients(field, false), trailingToken(field))).toBe(field);
  });

  it('removes one recipient, keeping any token in progress', () => {
    expect(removeRecipient('a@x.com, b@y.com, bo', 'a@x.com', false)).toBe('b@y.com, bo');
    expect(removeRecipient('a@x.com, b@y.com, bo', 'bo', false)).toBe('a@x.com, b@y.com, bo');
    expect(removeRecipient('a@x.com, b@y.com', 'a@x.com', true)).toBe('b@y.com');
    expect(removeRecipient('a@x.com', 'a@x.com', true)).toBe('');
  });

  it('normalises a seeded field into its finished recipients', () => {
    expect(normaliseRecipients(' a@x.com , b@y.com ,,')).toBe('a@x.com, b@y.com');
    expect(normaliseRecipients('a@x.com,b@y.com')).toBe('a@x.com, b@y.com');
    expect(normaliseRecipients('  ')).toBe('');
    expect(normaliseRecipients('')).toBe('');
  });
});
