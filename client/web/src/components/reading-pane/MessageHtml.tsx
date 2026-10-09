// SPDX-License-Identifier: GPL-3.0-only
// The message body: the sanitised HTML rides in a srcdoc iframe locked to an
// empty sandbox (no same origin, no popups, no scripts) under a csp that
// allows nothing to load but data images and inline styles, so the rewritten
// placeholders are defended in depth behind a barrier that holds even if one
// leaks through. The toggle rewrites the placeholders back to live remote
// references client-side, per message, and clears the blocked-count
// affordance.

/** The framed document's barrier: nothing loads but data images and styles. */
export const READING_CSP = "default-src 'none'; img-src data:; style-src 'unsafe-inline'";

/**
 * Rewrites the sanitiser's placeholders back to the remote references they
 * carry: an inert `data-astra-src` attribute becomes `src` again, and the
 * `data-astra-style-url:` prefix inside CSS is stripped.
 */
export function enableRemoteImages(html: string): string {
  return html.replaceAll('data-astra-src=', 'src=').replaceAll('data-astra-style-url:', '');
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
  const blocked = remoteImageCount > 0 && !showRemoteImages;
  const noun = remoteImageCount === 1 ? 'image' : 'images';
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
        title="Message body"
        sandbox=""
        csp={READING_CSP}
        srcDoc={doc}
        className="min-h-0 w-full flex-1 border-0 bg-background"
      />
    </div>
  );
}
