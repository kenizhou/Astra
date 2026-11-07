// SPDX-License-Identifier: GPL-3.0-only
// The assistant text's markdown renderer: a small subset (paragraphs,
// headings, lists, fenced code, inline code, emphasis, links) rendered
// STRAIGHT TO REACT ELEMENTS. There is no HTML string, no innerHTML, and
// no sanitiser to forget: raw HTML in the source stays literal text
// (React escapes every text node), script never executes, and images
// never render, so no remote load can ride a chat bubble. Links follow
// the composer's own allowlist discipline (http, https, mailto, the
// 2048 bound, no whitespace): a safe href renders as an anchor whose
// click is routed, never navigated, the reading pane's link discipline
// (mailto hands the raw link to the host's parse, everything else goes
// to the OS through the opener); any other scheme stays the literal
// bracket text.

import { Fragment, type ReactNode } from 'react';

import { openUrl } from '@tauri-apps/plugin-opener';

import { askMailtoOpen, isMailtoHref } from '@/features/composer/mailtoOpen';

/** The link target bound, the composer's finished-links contract. */
const LINK_MAX = 2048;

/** One inline run: literal text, a code span, emphasis, or a link. */
type Inline =
  | { inline: 'text'; text: string }
  | { inline: 'code'; text: string }
  | { inline: 'em'; text: string }
  | { inline: 'strong'; text: string }
  | { inline: 'link'; text: string; href: string };

/** The link shapes the renderer admits: `null` means the whole bracket
 *  text stays literal (the sandbox refuses the scheme, not the words). The
 *  composer allowlist's whole discipline: the two schemes, the 2048 bound,
 *  no whitespace or control characters, and something after the scheme. */
function linkHref(href: string): string | null {
  if (href.length > LINK_MAX || /[\s\u0000-\u001f\u007f]/.test(href)) {
    return null;
  }
  if (/^https?:\/\/.+/i.test(href) || /^mailto:.+@.+$/i.test(href)) {
    return href;
  }
  return null;
}

/** The inline scanner: one alternation, leftmost match wins; nothing
 *  nested (small renderer), nothing that parses a tag. A link target may
 *  nest one parenthesised pair so an adversarial `javascript:` target
 *  stays one match the allowlist can refuse whole. */
const INLINE_RE =
  /(`[^`\n]+`|\*\*[^*\n]+\*\*|\*[^*\n]+\*|__[^_\n]+__|_[^_\n]+_|\[[^\]\n]+\]\((?:[^()\s]|\([^()\s]*\))+\))/g;

/** Splits one text run into its inline shapes. */
export function parseInline(text: string): Inline[] {
  const runs: Inline[] = [];
  let at = 0;
  for (const match of text.matchAll(INLINE_RE)) {
    const found = match[0];
    const index = match.index ?? 0;
    // A picture's `![alt](src)` is never a link: the bang stays literal
    // and the whole bracket text with it, so no remote source renders.
    const bang = index > 0 && text[index - 1] === '!';
    if (bang) {
      if (index > at) {
        runs.push({ inline: 'text', text: text.slice(at, index - 1) });
      }
      runs.push({ inline: 'text', text: `${text[index - 1]}${found}` });
      at = index + found.length;
      continue;
    }
    if (index > at) {
      runs.push({ inline: 'text', text: text.slice(at, index) });
    }
    if (found.startsWith('`')) {
      runs.push({ inline: 'code', text: found.slice(1, -1) });
    } else if (found.startsWith('**') || found.startsWith('__')) {
      runs.push({ inline: 'strong', text: found.slice(2, -2) });
    } else if (found.startsWith('*') || found.startsWith('_')) {
      runs.push({ inline: 'em', text: found.slice(1, -1) });
    } else {
      const cut = found.indexOf('](');
      const href = linkHref(found.slice(cut + 2, -1));
      if (href === null) {
        runs.push({ inline: 'text', text: found });
      } else {
        runs.push({ inline: 'link', text: found.slice(1, cut), href });
      }
    }
    at = index + found.length;
  }
  if (at < text.length) {
    runs.push({ inline: 'text', text: text.slice(at) });
  }
  return runs;
}

/** One block: a paragraph, a heading, a fenced code block, or a list. */
type Block =
  | { block: 'paragraph'; text: string }
  | { block: 'heading'; level: number; text: string }
  | { block: 'code'; text: string }
  | { block: 'list'; ordered: boolean; items: string[] };

const HEADING_RE = /^(#{1,4})\s+(.*)$/;
const UL_RE = /^\s*[-*]\s+(.*)$/;
const OL_RE = /^\s*\d+[.)]\s+(.*)$/;

/** Splits the text into blocks; the shapes the subset admits and no
 *  others (quotes and tables stay paragraph text). */
export function parseBlocks(text: string): Block[] {
  const lines = text.split('\n');
  const blocks: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line === undefined) {
      break;
    }
    if (line.trim() === '') {
      i += 1;
      continue;
    }
    if (line.startsWith('```')) {
      const body: string[] = [];
      i += 1;
      for (;;) {
        const inner = lines[i];
        if (inner === undefined || inner.startsWith('```')) {
          break;
        }
        body.push(inner);
        i += 1;
      }
      i += 1;
      blocks.push({ block: 'code', text: body.join('\n') });
      continue;
    }
    const heading = HEADING_RE.exec(line);
    if (heading !== null && heading[1] !== undefined && heading[2] !== undefined) {
      blocks.push({ block: 'heading', level: heading[1].length, text: heading[2] });
      i += 1;
      continue;
    }
    const listRe = UL_RE.test(line) ? UL_RE : OL_RE.test(line) ? OL_RE : null;
    if (listRe !== null) {
      const items: string[] = [];
      for (;;) {
        const raw = lines[i];
        const item = raw === undefined ? null : listRe.exec(raw);
        if (item === null || item[1] === undefined) {
          break;
        }
        items.push(item[1]);
        i += 1;
      }
      blocks.push({ block: 'list', ordered: listRe === OL_RE, items });
      continue;
    }
    const para: string[] = [];
    for (;;) {
      const next = lines[i];
      if (
        next === undefined ||
        next.trim() === '' ||
        next.startsWith('```') ||
        HEADING_RE.test(next) ||
        UL_RE.test(next) ||
        OL_RE.test(next)
      ) {
        break;
      }
      para.push(next);
      i += 1;
    }
    blocks.push({ block: 'paragraph', text: para.join('\n') });
  }
  return blocks;
}

/** One link's click: routed, never navigated. The raw mailto link hands
 *  itself to the host's parse (the composer's ask); every other safe
 *  href goes to the OS through the opener plugin. */
function openMarkdownLink(href: string): void {
  if (isMailtoHref(href)) {
    void askMailtoOpen(href);
    return;
  }
  void openUrl(href);
}

/** Renders the inline runs of one text. Literal text rides a fragment,
 *  so the block element itself owns it (headings stay headings to the
 *  accessibility tree). */
function InlineRuns({ text }: { text: string }): ReactNode {
  return (
    <>
      {parseInline(text).map((run, at) => {
        switch (run.inline) {
          case 'text':
            return <Fragment key={at}>{run.text}</Fragment>;
          case 'code':
            return (
              <code key={at} className="rounded-[var(--radius-xs)] bg-hover px-1">
                {run.text}
              </code>
            );
          case 'em':
            return <em key={at}>{run.text}</em>;
          case 'strong':
            return <strong key={at}>{run.text}</strong>;
          case 'link':
            return (
              <a
                key={at}
                href={run.href}
                className="text-primary underline"
                onClick={(event) => {
                  event.preventDefault();
                  openMarkdownLink(run.href);
                }}
              >
                {run.text}
              </a>
            );
        }
      })}
    </>
  );
}

/** The assistant bubble's body: the markdown subset as elements. */
export function Markdown({ text }: { text: string }) {
  return (
    <>
      {parseBlocks(text).map((block, at) => {
        switch (block.block) {
          case 'paragraph':
            return (
              <p key={at}>
                <InlineRuns text={block.text} />
              </p>
            );
          case 'heading': {
            const Tag = `h${Math.min(block.level + 1, 5)}` as 'h2' | 'h3' | 'h4' | 'h5';
            return (
              <Tag key={at} className="font-medium">
                <InlineRuns text={block.text} />
              </Tag>
            );
          }
          case 'code':
            return (
              <pre key={at} className="overflow-x-auto rounded-[var(--radius-xs)] bg-hover px-2 py-1">
                <code>{block.text}</code>
              </pre>
            );
          case 'list': {
            const List = block.ordered ? 'ol' : 'ul';
            return (
              <List key={at} className="list-inside list-disc space-y-0.5 pl-1">
                {block.items.map((item, itemAt) => (
                  <li key={itemAt}>
                    <InlineRuns text={item} />
                  </li>
                ))}
              </List>
            );
          }
        }
      })}
    </>
  );
}
