// SPDX-License-Identifier: GPL-3.0-only
// The message body: the sanitised HTML rides in a sandboxed srcdoc iframe
// whose csp allows only data images and inline styles, remote placeholders
// stay inert until the toggle rewrites them, and the blocked count
// affordance clears once the reader opts in.
import { useState } from 'react';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import {
  MessageHtml,
  READING_CSP_GATED,
  READING_CSP_REMOTE,
  enableRemoteImages,
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
function Harness({ html = sanitisedHtml, count = 2 }: { html?: string; count?: number }) {
  const [show, setShow] = useState(false);
  return (
    <MessageHtml
      html={html}
      remoteImageCount={count}
      showRemoteImages={show}
      onShowRemoteImages={() => setShow(true)}
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

describe('the message body', () => {
  it('renders the sanitised html untouched in the srcdoc', () => {
    render(<Harness />);

    expect(frameDoc()).toBe(sanitisedHtml);
  });

  it('locks the iframe down: empty sandbox, gated csp allowing only data images', () => {
    render(<Harness />);

    const sandbox = frame().getAttribute('sandbox') ?? '';
    expect(sandbox).toBe('');
    expect(sandbox).not.toContain('allow-same-origin');
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

    // The doc is the snapshot's sanitised HTML untouched: the attribute value
    // carries the quote and the closing tag verbatim, so no client-side
    // escape or breakout creeps in between snapshot and frame.
    expect(frameDoc()).toBe(hostile);
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
