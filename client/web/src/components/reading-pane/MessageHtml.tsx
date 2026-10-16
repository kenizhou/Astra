// SPDX-License-Identifier: GPL-3.0-only
// The message body: the sanitised HTML rides in a srcdoc iframe. The csp
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
// in the system browser through the opener plugin, and the context menu
// offers open and copy beside Copy and Select All.

import { useEffect, useRef, useState } from 'react';

import { openUrl } from '@tauri-apps/plugin-opener';

import { ContextMenu, type ContextMenuItem } from '@/components/ui/ContextMenu';

/** The framed document's barrier while remote references stay gated. */
export const READING_CSP_GATED =
  "default-src 'none'; img-src data:; style-src 'unsafe-inline'";

/** The barrier once the reader opts in: remote images and remote CSS join,
 * `default-src 'none'` unchanged so no scripts, frames, or fetches load. */
export const READING_CSP_REMOTE =
  "default-src 'none'; img-src data: https:; style-src 'unsafe-inline' https:";

/**
 * Rewrites the sanitiser's placeholders back to the remote references they
 * carry: an inert `data-astra-src` attribute becomes `src` again, and the
 * `data-astra-style-url:` prefix inside CSS is stripped.
 */
export function enableRemoteImages(html: string): string {
  return html.replaceAll('data-astra-src=', 'src=').replaceAll('data-astra-style-url:', '');
}

/** The body menu's items over whatever the click landed on. */
function bodyItems(link: string | null, selection: string, selectAll: () => void): ContextMenuItem[] {
  const items: ContextMenuItem[] = [];
  if (link !== null) {
    items.push(
      { label: 'Open link', onSelect: () => void openUrl(link) },
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
 *  anchor clicks route to the system browser. The listeners attach on the
 *  frame's load, not at mount: the srcdoc document replaces the initial
 *  empty one asynchronously, and handlers on the replaced document die
 *  with it. */
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
            void openUrl(href);
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
}

export function MessageHtml({
  html,
  remoteImageCount,
  showRemoteImages,
  onShowRemoteImages,
}: MessageHtmlProps) {
  const doc = showRemoteImages ? enableRemoteImages(html) : html;
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
