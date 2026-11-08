// SPDX-License-Identifier: GPL-3.0-only
// The message body: the sanitised HTML rides in a srcdoc iframe (same-origin
// but scriptless, so the parent can wire its affordances) whose csp allows
// only data images and inline styles, remote placeholders stay inert until
// the toggle rewrites them, and the blocked count affordance clears once
// the reader opts in.
import { useState } from 'react';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: a mailto anchor routes into the composer ask only
  // inside Tauri, so the jsdom test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const opener = vi.hoisted(() => ({ openUrl: vi.fn() }));

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: opener.openUrl,
}));

const mailto = vi.hoisted(() => ({ askMailtoOpen: vi.fn() }));

vi.mock('@/features/composer/mailtoOpen', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  askMailtoOpen: mailto.askMailtoOpen,
}));

import {
  MessageHtml,
  READING_CSP_GATED,
  READING_CSP_REMOTE,
  READING_ZOOM_BASE_PX,
  enableRemoteImages,
  wrapBodyDocument,
} from './MessageHtml';

/** The sanitised body the Rust gate emits for the rich message: one inline
 * data URI left alone, one remote src behind a placeholder, one remote CSS
 * url behind the style placeholder scheme. */
const sanitisedHtml =
  '<p>The launch is ready.</p>' +
  '<img src="data:image/png;base64,aGVsbG8=">' +
  '<img data-astra-src="https://tracker.example/p.png">' +
  '<style>body{background:url(data-astra-style-url:https://x.example/a.png)}</style>';

/** Renders the body with the toggle wired to local state, the store's stand-in. */
function Harness({
  html = sanitisedHtml,
  count = 2,
  zoom = 'default',
}: {
  html?: string;
  count?: number;
  zoom?: 'small' | 'default' | 'large';
}) {
  const [show, setShow] = useState(false);
  return (
    <MessageHtml
      html={html}
      remoteImageCount={count}
      showRemoteImages={show}
      onShowRemoteImages={() => setShow(true)}
      zoom={zoom}
    />
  );
}

function frame(): HTMLElement {
  return screen.getByTitle('Message body');
}

function frameDoc(): string {
  return frame().getAttribute('srcdoc') ?? '';
}

afterEach(cleanup);

beforeEach(() => {
  opener.openUrl.mockReset();
  opener.openUrl.mockResolvedValue(undefined);
  mailto.askMailtoOpen.mockReset();
  mailto.askMailtoOpen.mockResolvedValue(undefined);
});

describe('the message body', () => {
  it('wraps the sanitised html in a full document, the fragment riding verbatim', () => {
    render(<Harness />);

    expect(frameDoc()).toContain(sanitisedHtml);
    expect(frameDoc().startsWith('<!DOCTYPE html>')).toBe(true);
  });

  it('locks the iframe down: same-origin but scriptless sandbox, gated csp allowing only data images', () => {
    render(<Harness />);

    const sandbox = frame().getAttribute('sandbox') ?? '';
    // allow-same-origin lets the parent wire the body menu and link
    // opening; scripts can still never run, and popups stay shut.
    expect(sandbox).toBe('allow-same-origin');
    expect(sandbox).not.toContain('allow-scripts');
    expect(sandbox).not.toContain('allow-popups');
    expect(frame().getAttribute('csp')).toBe(READING_CSP_GATED);
    expect(READING_CSP_GATED).toBe(
      "default-src 'none'; img-src data:; style-src 'unsafe-inline'",
    );
  });

  it('keeps every remote reference out of the doc and offers the blocked count', () => {
    render(<Harness />);

    expect(frameDoc()).not.toContain(' src="http');
    expect(frameDoc()).toContain('data-astra-src="https://tracker.example/p.png"');
    expect(frameDoc()).toContain('url(data-astra-style-url:https://x.example/a.png)');
    expect(screen.getByText('2 remote images blocked.')).toBeTruthy();
  });

  it('flips the placeholders to live references on toggle and clears the count', () => {
    render(<Harness />);

    fireEvent.click(screen.getByRole('button', { name: 'Show remote images' }));

    expect(frameDoc()).toContain('src="https://tracker.example/p.png"');
    expect(frameDoc()).toContain('url(https://x.example/a.png)');
    expect(frameDoc()).not.toContain('data-astra-src');
    expect(frameDoc()).not.toContain('data-astra-style-url');
    expect(screen.queryByText('2 remote images blocked.')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Show remote images' })).toBeNull();
  });

  it('widens the csp with the opt-in: remote images and styles join, defaults never lift', () => {
    render(<Harness />);

    fireEvent.click(screen.getByRole('button', { name: 'Show remote images' }));

    expect(frame().getAttribute('csp')).toBe(READING_CSP_REMOTE);
    expect(READING_CSP_REMOTE).toBe(
      "default-src 'none'; img-src data: https:; style-src 'unsafe-inline' https:",
    );
    // The csp and the srcdoc transform ride the same state: a widened csp
    // never coexists with gated placeholders, and vice versa.
    expect(frameDoc()).not.toContain('data-astra-src');
  });

  it('carries hostile quotes and markup verbatim into the srcdoc with no breakout', () => {
    const hostile = '<p>"> quoted</p><p></iframe><script>never runs</script></p>';
    render(<Harness html={hostile} count={0} />);

    // The fragment rides the wrapper's body untouched: the attribute value
    // carries the quote and the closing tag verbatim, so no client-side
    // escape or breakout creeps in between snapshot and frame.
    expect(frameDoc()).toContain(hostile);
  });

  it('draws no affordance when nothing is blocked', () => {
    render(<Harness html="<p>Plain.</p>" count={0} />);

    expect(screen.queryByRole('button', { name: 'Show remote images' })).toBeNull();
    expect(screen.queryByText(/blocked/)).toBeNull();
  });

  it('singularises the count for one blocked image', () => {
    render(<Harness html='<img data-astra-src="https://x.example/a.png">' count={1} />);

    expect(screen.getByText('1 remote image blocked.')).toBeTruthy();
  });

  it('exposes the toggle rewrite as a pure transform', () => {
    expect(enableRemoteImages(sanitisedHtml)).toBe(
      '<p>The launch is ready.</p>' +
        '<img src="data:image/png;base64,aGVsbG8=">' +
        '<img src="https://tracker.example/p.png">' +
        '<style>body{background:url(https://x.example/a.png)}</style>',
    );
  });
});

describe('the reading zoom wrapper', () => {
  it('scales the document base font-size by the standing step', () => {
    render(<Harness zoom="small" />);
    expect(frameDoc()).toContain(`html{font-size:${READING_ZOOM_BASE_PX.small}px}`);

    cleanup();
    render(<Harness zoom="large" />);
    expect(frameDoc()).toContain(`html{font-size:${READING_ZOOM_BASE_PX.large}px`);
  });

  it('holds three steps with the default matching the frame document the fragment always rendered as', () => {
    expect(READING_ZOOM_BASE_PX).toEqual({ small: 13, default: 16, large: 20 });
  });

  it('carries the viewport meta the sender mobile rules read', () => {
    render(<Harness />);

    expect(frameDoc()).toContain('<meta name="viewport" content="width=device-width, initial-scale=1">');
  });

  it('clamps fixed-width content to the viewport so the body reflows at every pane width', () => {
    render(<Harness />);

    const doc = frameDoc();
    expect(doc).toContain('body>*{max-width:100% !important}');
    expect(doc).toContain('img,video,table{max-width:100% !important}');
    expect(doc).toContain('img,video{height:auto}');
  });

  it('exposes the wrapper as a pure transform over the sanitised fragment', () => {
    const doc = wrapBodyDocument('<p>Body.</p>', 'large');
    expect(doc).toBe(
      '<!DOCTYPE html><html><head>' +
        '<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">' +
        '<style>html{font-size:20px}body{margin:0}body>*{max-width:100% !important}' +
        'img,video,table{max-width:100% !important}img,video{height:auto}</style>' +
        '</head><body><p>Body.</p></body></html>',
    );
  });
});

describe('the body menu', () => {
  // jsdom never parses the srcdoc into the frame, so the tests write the
  // body in themselves; the component's listeners ride the document
  // object, which a document.open rewrite keeps.
  function frameDocument(html: string): Document {
    const doc = (screen.getByTitle('Message body') as HTMLIFrameElement).contentDocument!;
    doc.open();
    doc.write(html);
    doc.close();
    return doc;
  }

  it('opens on a right-click inside the frame', () => {
    render(<Harness html='<p>The launch is ready.</p>' count={0} />);

    fireEvent.contextMenu(frameDocument('<p>The launch is ready.</p>').body);

    expect(screen.getByRole('menu')).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Select All' })).toBeTruthy();
    // No anchor under the click: no link items.
    expect(screen.queryByRole('menuitem', { name: 'Open link' })).toBeNull();
  });

  it('offers the link items over an anchor', () => {
    render(<Harness html='<p><a href="https://example.test/x">read more</a></p>' count={0} />);

    fireEvent.contextMenu(frameDocument('<p><a href="https://example.test/x">read more</a></p>').querySelector('a')!);

    expect(screen.getByRole('menuitem', { name: 'Open link' })).toBeTruthy();
    expect(screen.getByRole('menuitem', { name: 'Copy link' })).toBeTruthy();
  });

  it('copies the link href from the menu', () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    render(<Harness html='<p><a href="https://example.test/x">read more</a></p>' count={0} />);

    fireEvent.contextMenu(frameDocument('<p><a href="https://example.test/x">read more</a></p>').querySelector('a')!);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Copy link' }));

    expect(writeText).toHaveBeenCalledWith('https://example.test/x');
  });

  it('opens a clicked link in the system browser, never in the frame', () => {
    render(<Harness html='<p><a href="https://example.test/x">read more</a></p>' count={0} />);

    fireEvent.click(frameDocument('<p><a href="https://example.test/x">read more</a></p>').querySelector('a')!);

    expect(opener.openUrl).toHaveBeenCalledWith('https://example.test/x');
  });

  it('routes a clicked mailto anchor into the composer ask, never the OS mail app', () => {
    const link = 'mailto:ada@example.test?subject=Lunch';
    render(<Harness html={`<p><a href="${link}">write in</a></p>`} count={0} />);

    fireEvent.click(frameDocument(`<p><a href="${link}">write in</a></p>`).querySelector('a')!);

    expect(mailto.askMailtoOpen).toHaveBeenCalledWith(link);
    expect(opener.openUrl).not.toHaveBeenCalled();
  });

  it('opens the menu link in the system browser', () => {
    render(<Harness html='<p><a href="https://example.test/x">read more</a></p>' count={0} />);

    fireEvent.contextMenu(frameDocument('<p><a href="https://example.test/x">read more</a></p>').querySelector('a')!);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Open link' }));

    expect(opener.openUrl).toHaveBeenCalledWith('https://example.test/x');
  });

  it("routes the menu's mailto open through the composer ask", () => {
    const link = 'mailto:ada@example.test';
    render(<Harness html={`<p><a href="${link}">write in</a></p>`} count={0} />);

    fireEvent.contextMenu(frameDocument(`<p><a href="${link}">write in</a></p>`).querySelector('a')!);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Open link' }));

    expect(mailto.askMailtoOpen).toHaveBeenCalledWith(link);
    expect(opener.openUrl).not.toHaveBeenCalled();
  });
});
