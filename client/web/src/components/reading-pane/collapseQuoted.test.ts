// SPDX-License-Identifier: GPL-3.0-only
// The quote fold as a pure transform: exact output pins for the trailing
// chain, the attribution line, the Gmail shape, the safety no-ops, and the
// remote placeholders riding the fold untouched.
import { describe, expect, it } from 'vitest';

import { collapseQuoted } from './collapseQuoted';

const OPEN = '<details class="astra-quote"><summary>Show quoted text</summary>';

describe('collapseQuoted', () => {
  it('folds a trailing blockquote chain into a collapsed details', () => {
    const html = '<p>My reply</p><blockquote><p>old</p></blockquote>';
    expect(collapseQuoted(html)).toEqual({
      html: `<p>My reply</p>${OPEN}<blockquote><p>old</p></blockquote></details>`,
      collapsed: true,
    });
  });

  it('folds a chain of several blockquotes as one tail', () => {
    const html =
      '<p>My reply</p><blockquote><p>first</p></blockquote><blockquote><p>second</p></blockquote>';
    expect(collapseQuoted(html)).toEqual({
      html:
        `<p>My reply</p>${OPEN}` +
        '<blockquote><p>first</p></blockquote><blockquote><p>second</p></blockquote>' +
        '</details>',
      collapsed: true,
    });
  });

  it('folds the attribution line inside the details', () => {
    const attribution = '<p>On Tue, Sep 30, 2026 at 10:00 AM A &lt;a@x.com&gt; wrote:</p>';
    const html = `<p>My reply</p>${attribution}<blockquote><p>old</p></blockquote>`;
    expect(collapseQuoted(html)).toEqual({
      html: `<p>My reply</p>${OPEN}${attribution}<blockquote><p>old</p></blockquote></details>`,
      collapsed: true,
    });
  });

  it('folds a div attribution line too', () => {
    const attribution = '<div class="quote">On Mon, 1 Nov 2026, B wrote:</div>';
    const html = `<p>My reply</p>${attribution}<blockquote>old</blockquote>`;
    expect(collapseQuoted(html)).toEqual({
      html: `<p>My reply</p>${OPEN}${attribution}<blockquote>old</blockquote></details>`,
      collapsed: true,
    });
  });

  it('folds a trailing gmail_quote block with its nested markup', () => {
    const gmail =
      '<div class="gmail_quote"><div dir="ltr">On Tue, C wrote:</div>' +
      '<blockquote class="gmail_quote"><p>old</p></blockquote></div>';
    const html = `<p>My reply</p>${gmail}`;
    expect(collapseQuoted(html)).toEqual({
      html: `<p>My reply</p>${OPEN}${gmail}</details>`,
      collapsed: true,
    });
  });

  it('folds a trailing gmail_quote blockquote shape', () => {
    const gmail = '<blockquote class="gmail_quote"><p>old</p></blockquote>';
    const html = `<p>My reply</p>${gmail}`;
    expect(collapseQuoted(html)).toEqual({
      html: `<p>My reply</p>${OPEN}${gmail}</details>`,
      collapsed: true,
    });
  });

  it('leaves a mid-body blockquote unchanged', () => {
    const html = '<blockquote><p>q</p></blockquote><p>after</p>';
    expect(collapseQuoted(html)).toEqual({ html, collapsed: false });
  });

  it('leaves a mid-body gmail_quote unchanged, never swallowing the words after it', () => {
    const html = '<p>new</p><div class="gmail_quote"><div>old</div></div><p>after</p>';
    expect(collapseQuoted(html)).toEqual({ html, collapsed: false });
  });

  it('leaves a quote-free body unchanged', () => {
    const html = '<p>plain</p>';
    expect(collapseQuoted(html)).toEqual({ html, collapsed: false });
  });

  it('never collapses a body that is entirely one quote', () => {
    const html = '<blockquote><p>only</p></blockquote>';
    expect(collapseQuoted(html)).toEqual({ html, collapsed: false });
  });

  it('never collapses a body that is entirely one gmail_quote', () => {
    const html = '<div class="gmail_quote"><p>only</p></div>';
    expect(collapseQuoted(html)).toEqual({ html, collapsed: false });
  });

  it('keeps the remote-image placeholders intact through the fold', () => {
    const quote =
      '<blockquote><img data-astra-src="https://x.example/a.png">' +
      '<style>body{background:url(data-astra-style-url:https://x.example/b.png)}</style></blockquote>';
    const html = `<p>My reply</p>${quote}`;
    expect(collapseQuoted(html)).toEqual({
      html: `<p>My reply</p>${OPEN}${quote}</details>`,
      collapsed: true,
    });
  });

  it('wraps the fragment verbatim, hostile markup and all', () => {
    const hostile = '<blockquote><p>"> quoted</p><p></iframe><script>never runs</script></p></blockquote>';
    const html = `<p>My reply</p>${hostile}`;
    expect(collapseQuoted(html)).toEqual({
      html: `<p>My reply</p>${OPEN}${hostile}</details>`,
      collapsed: true,
    });
  });

  it('carries no open attribute, so the fold stands collapsed', () => {
    const out = collapseQuoted('<p>My reply</p><blockquote>old</blockquote>');
    expect(out.html).toContain('<details class="astra-quote">');
    expect(out.html).not.toMatch(/<details[^>]*\bopen\b/i);
  });
});