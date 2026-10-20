// SPDX-License-Identifier: GPL-3.0-only
// The send-time link check: the document's link targets are validated
// against the core's allowlist (http, https, mailto, the 2048 bound, no
// whitespace or control characters, something after the scheme) before the
// submit, and any that degrade are named in one sentence under the
// toolbar. The sentence never blocks the send; the core's degradation to
// plain text stays the send's own floor.
import { describe, expect, it } from 'vitest';

import type { ComposerDocument } from '@/services/ipc';

import { degradingLinkTargets, degradationSentence } from './sendLinks';

/** One plain run carrying `link`, exactly as the wire spells it. */
function documentWithLinks(links: (string | null)[]): ComposerDocument {
  return {
    blocks: [
      {
        Paragraph: {
          content: links.map((link) => ({
            Text: {
              text: 'run',
              bold: false,
              italic: false,
              underline: false,
              font_size: null,
              color: null,
              highlight: null,
              link,
            },
          })),
        },
      },
    ],
    attachments: [],
  };
}

describe('the allowlist mirror', () => {
  it('keeps the schemes the core allows', () => {
    expect(degradingLinkTargets(documentWithLinks(['https://example.test', 'http://a.test', 'mailto:a@b.test']))).toEqual([]);
  });

  it('names a refused scheme, an overlong target, and a whitespace target', () => {
    expect(degradingLinkTargets(documentWithLinks(['javascript:alert(1)']))).toEqual([
      'javascript:alert(1)',
    ]);
    expect(degradingLinkTargets(documentWithLinks(['https://exa mple.test']))).toEqual([
      'https://exa mple.test',
    ]);
    expect(degradingLinkTargets(documentWithLinks(['https://']))).toEqual(['https://']);
  });

  it('names an over-2048-byte target once', () => {
    const long = `https://example.test/${'a'.repeat(2040)}`;
    expect(long.length).toBeGreaterThan(2048);
    expect(degradingLinkTargets(documentWithLinks([long, 'https://fine.test']))).toEqual([long]);
  });

  it('names each degraded target once, in document order', () => {
    expect(
      degradingLinkTargets(documentWithLinks(['ftp://a.test', 'https://b.test', 'javascript:c'])),
    ).toEqual(['ftp://a.test', 'javascript:c']);
  });
});

describe('the sentence', () => {
  it('names the targets that degrade', () => {
    const sentence = degradationSentence(['javascript:alert(1)', 'ftp://a.test']);
    expect(sentence).toContain('javascript:alert(1)');
    expect(sentence).toContain('ftp://a.test');
  });

  it('is null when every target stands', () => {
    expect(degradationSentence([])).toBeNull();
  });
});
