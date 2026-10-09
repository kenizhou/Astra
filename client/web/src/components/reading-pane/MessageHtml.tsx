// SPDX-License-Identifier: GPL-3.0-only
// The message body: the sanitised HTML rides in a srcdoc iframe locked to an
// empty sandbox (no same origin, no popups, no scripts). The csp widens with
// the reader's opt-in, in step with the placeholder rewrite the toggle runs:
// gated, nothing loads but data images and inline styles; opted in, remote
// images and remote CSS join. The Rust-side placeholder rewrite is the
// primary barrier; the csp is defence in depth holding even if a reference
// leaks through, and `default-src 'none'` never lifts, so no scripts, frames,
// or fetches load in either state.

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
        csp={csp}
        srcDoc={doc}
        className="min-h-0 w-full flex-1 border-0 bg-background"
      />
    </div>
  );
}
