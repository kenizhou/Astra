// SPDX-License-Identifier: GPL-3.0-only
// Folds the trailing quoted original of a reply into a native details
// expander, so the reading pane opens on the new words. The fold wraps the
// sanitised fragment verbatim after the Rust pass: the injected markup is a
// fixed constant, and a body without a trailing quote, or one that is
// entirely a quote, rides unchanged.

export interface CollapseResult {
  html: string;
  collapsed: boolean;
}

const TRAILING_QUOTES = /(?:\s*(?:<blockquote\b[^>]*>[\s\S]*?<\/blockquote>))+\s*$/i;
const ATTRIBUTION = /(?:<p[^>]*>|<div[^>]*>)\s*On [\s\S]{0,400}?wrote:\s*<\/(?:p|div)>\s*$/i;
const GMAIL_QUOTE = /<(div|blockquote)\b[^>]*\bclass="[^"]*\bgmail_quote\b[^"]*"[^>]*>/gi;

/** Whether the element opening at `start` is the whole tail: one balanced
 *  element with only whitespace after it, so a mid-body quote never
 *  swallows the words that follow it. */
function isWholeTail(html: string, start: number, tag: string): boolean {
  const open = new RegExp(`<${tag}\\b`, 'gi');
  const close = new RegExp(`</${tag}>`, 'gi');
  open.lastIndex = start;
  close.lastIndex = start;
  let depth = 0;
  for (;;) {
    const nextOpen = open.exec(html);
    const nextClose = close.exec(html);
    if (nextClose === null) {
      return false;
    }
    if (nextOpen === null) {
      // A failed exec rewinds lastIndex to zero; park it at the end so the
      // remaining closes are never recounted against fresh opens.
      open.lastIndex = html.length;
    } else if (nextOpen.index < nextClose.index) {
      depth += 1;
      // exec advances past the peeked close; hold it for the next round.
      close.lastIndex = nextClose.index;
      continue;
    } else {
      open.lastIndex = nextOpen.index;
    }
    depth -= 1;
    if (depth === 0) {
      return html.slice(nextClose.index + nextClose[0].length).trim() === '';
    }
  }
}

/** The first gmail_quote opening tag whose element is the whole tail: the
 *  outer container of a nested Gmail quote, never an inner block. */
function gmailTailStart(html: string): number | null {
  for (const m of html.matchAll(GMAIL_QUOTE)) {
    if (isWholeTail(html, m.index!, m[1]!.toLowerCase())) {
      return m.index!;
    }
  }
  return null;
}

export function collapseQuoted(html: string): CollapseResult {
  let cut: number | null = null;
  const tail = html.match(TRAILING_QUOTES);
  if (tail !== null && tail[0].length < html.length) {
    cut = html.length - tail[0].length;
    const attribution = html.slice(0, cut).match(ATTRIBUTION);
    if (attribution !== null) {
      cut -= attribution[0].length;
    }
  } else {
    cut = gmailTailStart(html);
  }
  if (cut === null || cut === 0) {
    return { html, collapsed: false };
  }
  return {
    html: `${html.slice(0, cut)}<details class="astra-quote"><summary>Show quoted text</summary>${html.slice(cut)}</details>`,
    collapsed: true,
  };
}