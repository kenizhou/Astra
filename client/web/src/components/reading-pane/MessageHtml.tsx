// SPDX-License-Identifier: GPL-3.0-only
// The message body: the sanitised HTML rides in a srcdoc iframe, wrapped in
// a document of the reading zoom's own. The wrapper's base font-size scales
// with the standing step, its viewport meta lets the sender's mobile rules
// read the frame's width, and its clamp rules hold every fixed-width
// element to the viewport so the body reflows live during resize; the
// sanitised fragment itself rides the wrapper's body verbatim. The csp
// widens with the reader's opt-in, in step with the placeholder rewrite
// the toggle runs: gated, nothing loads but data images and inline
// styles; opted in, remote images and remote CSS join. The Rust-side
// placeholder rewrite is the primary barrier; the csp is defence in
// depth holding even if a reference leaks through, and `default-src
// 'none'` never lifts, so no scripts, frames, or fetches load in either
// state. The frame is same-origin but scriptless
// (`sandbox="allow-same-origin"`), the config kylins' renderer documents:
// sanitisation upstream, no script execution here, and the parent free
// to attach the listeners the body's own affordances need: links open
// in the system browser through the opener plugin, a mailto link routed
// into the composer's ask instead, and the context menu offers open and
// copy beside Copy and Select All.

import { useEffect, useRef, useState } from 'react';

import { openUrl } from '@tauri-apps/plugin-opener';

import { ContextMenu, type ContextMenuItem } from '@/components/ui/ContextMenu';
import { isTauri } from '@/events/surfaceEvents';
import { askMailtoOpen, isMailtoHref } from '@/features/composer/mailtoOpen';
import type { ReadingZoomStep } from '@/services/appearance';

/** The framed document's barrier while remote references stay gated. */
export const READING_CSP_GATED =
  "default-src 'none'; img-src data:; style-src 'unsafe-inline'";

/** The barrier once the reader opts in: remote images and remote CSS join,
 * `default-src 'none'` unchanged so no scripts, frames, or fetches load. */
export const READING_CSP_REMOTE =
  "default-src 'none'; img-src data: https:; style-src 'unsafe-inline' https:";

/**
 * The reading body's base sizes, one per zoom step. The default is the size
 * the frame document's fragment always rendered at (the browser's own
 * sixteen-pixel medium), so the standing default changes nothing and the
 * steps move around it.
 */
export const READING_ZOOM_BASE_PX: Record<ReadingZoomStep, number> = {
  small: 13,
  default: 16,
  large: 20,
};

/** The wrapper's stylesheet: the base font-size the step names, and the
 *  clamp that holds fixed-width content to the viewport, so a table or a
 *  hard pixel width reflows at every pane width, live during resize (the
 *  rules are CSS, so the browser reapplies them per layout pass). */
const WRAPPER_STYLE = (zoom: ReadingZoomStep): string =>
  `html{font-size:${READING_ZOOM_BASE_PX[zoom]}px}` +
  'body{margin:0}' +
  'body>*{max-width:100% !important}' +
  'img,video,table{max-width:100% !important}' +
  'img,video{height:auto}';

/**
 * Wraps the sanitised fragment in the zoom document: a viewport meta the
 * sender's mobile rules read (a newsletter laid out against the frame's own
 * width applies its own rules at every width), the base stylesheet above,
 * and the fragment itself in the body, verbatim, with no escape or rewrite
 * between the snapshot and the frame.
 */
export function wrapBodyDocument(html: string, zoom: ReadingZoomStep): string {
  return (
    '<!DOCTYPE html><html><head>' +
    '<meta charset="utf-8">' +
    '<meta name="viewport" content="width=device-width, initial-scale=1">' +
    `<style>${WRAPPER_STYLE(zoom)}</style>` +
    `</head><body>${html}</body></html>`
  );
}

/**
 * Rewrites the sanitiser's placeholders back to the remote references they
 * carry: an inert `data-astra-src` attribute becomes `src` again, and the
 * `data-astra-style-url:` prefix inside CSS is stripped.
 */
export function enableRemoteImages(html: string): string {
  return html.replaceAll('data-astra-src=', 'src=').replaceAll('data-astra-style-url:', '');
}

/**
 * Routes one body link: a mailto link opens the prefilled composer through
 * the host's parse (the link is never read here), every other scheme still
 * goes to the OS, and outside Tauri the OS mail app takes the mailto link
 * as it always did.
 */
function openBodyLink(href: string): void {
  if (isTauri && isMailtoHref(href)) {
    void askMailtoOpen(href);
    return;
  }
  void openUrl(href);
}

/** The body menu's items over whatever the click landed on. */
function bodyItems(link: string | null, selection: string, selectAll: () => void): ContextMenuItem[] {
  const items: ContextMenuItem[] = [];
  if (link !== null) {
    items.push(
      { label: 'Open link', onSelect: () => openBodyLink(link) },
      { label: 'Copy link', onSelect: () => void navigator.clipboard?.writeText(link) },
      { separator: true },
    );
  }
  items.push(
    {
      label: 'Copy',
      disabled: selection === '',
      title: selection === '' ? 'Select some text to copy' : undefined,
      description: selection === '' ? 'Select some text to copy' : undefined,
      onSelect: () => void navigator.clipboard?.writeText(selection),
    },
    { label: 'Select All', onSelect: selectAll },
  );
  return items;
}

/** Wires the body menu into one frame document: contextmenu opens it,
 *  anchor clicks route to the composer or the system browser. The listeners
 *  attach on the frame's load, not at mount: the srcdoc document replaces
 *  the initial empty one asynchronously, and handlers on the replaced
 *  document die with it. */
function useBodyFrame(doc: string) {
  const frameRef = useRef<HTMLIFrameElement>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; link: string | null; selection: string } | null>(null);

  useEffect(() => {
    const frame = frameRef.current;
    if (frame === null) {
      return;
    }

    const attach = () => {
      const frameDoc = frame.contentDocument;
      if (frameDoc == null) {
        return;
      }
      const onClick = (event: MouseEvent) => {
        const anchor = (event.target as Element).closest('a');
        if (anchor !== null) {
          event.preventDefault();
          const href = anchor.getAttribute('href');
          if (href !== null && href !== '') {
            openBodyLink(href);
          }
        }
      };
      const onContextMenu = (event: MouseEvent) => {
        event.preventDefault();
        const anchor = (event.target as Element).closest('a');
        const frameBox = frame.getBoundingClientRect();
        setMenu({
          x: frameBox.left + event.clientX,
          y: frameBox.top + event.clientY,
          link: anchor?.getAttribute('href') ?? null,
          selection: frameDoc.getSelection()?.toString() ?? '',
        });
      };
      frameDoc.addEventListener('click', onClick);
      frameDoc.addEventListener('contextmenu', onContextMenu);
      return () => {
        frameDoc.removeEventListener('click', onClick);
        frameDoc.removeEventListener('contextmenu', onContextMenu);
      };
    };

    let detach = attach();
    const onLoad = () => {
      detach?.();
      detach = attach();
    };
    frame.addEventListener('load', onLoad);
    return () => {
      frame.removeEventListener('load', onLoad);
      detach?.();
    };
  }, [doc]);

  const selectAll = () => {
    const frameDoc = frameRef.current?.contentDocument;
    if (frameDoc != null) {
      frameDoc.getSelection()?.selectAllChildren(frameDoc.body);
    }
  };

  return { frameRef, menu, selectAll, closeMenu: () => setMenu(null) };
}

interface MessageHtmlProps {
  /** The sanitised HTML body, every remote reference still a placeholder. */
  html: string;
  /** How many remote references are blocked until the reader opts in. */
  remoteImageCount: number;
  /** Whether the reader opted into remote images for this message. */
  showRemoteImages: boolean;
  /** Records the opt-in. */
  onShowRemoteImages: () => void;
  /** The standing zoom step, wrapping the body's document. */
  zoom?: ReadingZoomStep;
}

export function MessageHtml({
  html,
  remoteImageCount,
  showRemoteImages,
  onShowRemoteImages,
  zoom = 'default',
}: MessageHtmlProps) {
  const doc = wrapBodyDocument(
    showRemoteImages ? enableRemoteImages(html) : html,
    zoom,
  );
  const csp = showRemoteImages ? READING_CSP_REMOTE : READING_CSP_GATED;
  const blocked = remoteImageCount > 0 && !showRemoteImages;
  const noun = remoteImageCount === 1 ? 'image' : 'images';
  const { frameRef, menu, selectAll, closeMenu } = useBodyFrame(doc);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {blocked && (
        <div className="flex items-center justify-between gap-2 border-b border-border bg-muted px-3 py-1.5 text-xs text-muted-foreground">
          <span>
            {remoteImageCount} remote {noun} blocked.
          </span>
          <button
            type="button"
            onClick={onShowRemoteImages}
            className="shrink-0 rounded-[var(--radius-xs)] px-2 py-1 font-medium text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Show remote images
          </button>
        </div>
      )}
      <iframe
        ref={frameRef}
        title="Message body"
        sandbox="allow-same-origin"
        csp={csp}
        srcDoc={doc}
        className="min-h-0 w-full flex-1 border-0 bg-background"
      />
      {menu !== null && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={bodyItems(menu.link, menu.selection, selectAll)}
          onClose={closeMenu}
        />
      )}
    </div>
  );
}
