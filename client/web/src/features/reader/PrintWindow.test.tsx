// SPDX-License-Identifier: GPL-3.0-only
// The print window's root: it hydrates the reading snapshot for its message
// and prints itself (window.print(), the platform dialog the webview opens),
// the document being the sanitised reading view reused verbatim: the
// subject, the header lines, the body at the standing zoom, and the
// attachment names. The remote placeholders stay inert: print loads no
// remote images, and the window closes once the dialog settles.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  settings: vi.fn(),
  reading: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  settings: ipc.settings,
  reading: ipc.reading,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

const currentWindow = vi.hoisted(() => ({ close: vi.fn() }));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: currentWindow.close,
  }),
}));

import type { ReadingSnapshot } from '@/services/ipc';
import { READING_CSP_GATED, wrapBodyDocument } from '@/components/reading-pane/MessageHtml';
import { useAppearanceStore } from '@/services/appearance';
import { useReadingStore } from '@/stores/readingStore';

import { PrintWindow } from './PrintWindow';

const message = { account: 'work', key: 'imap:v1:u42' };

/** The sanitised body the Rust gate emits: the placeholders stay, so a
 * printed page never loads a remote image. */
const sanitisedHtml =
  '<p>The launch is ready.</p><img data-astra-src="https://tracker.example/p.png">';

/** The open message's reading snapshot. */
function readingSnapshot(over: Partial<ReadingSnapshot> = {}): ReadingSnapshot {
  return {
    message,
    subject: 'Launch',
    from_name: 'Ada Lovelace',
    from_address: 'ada@example.test',
    sent_at: '2026-10-09T08:30:00Z',
    html: sanitisedHtml,
    plain: 'The launch is ready.',
    attachments: [
      { id: 1, file_name: 'report.pdf', media_type: 'application/pdf', size: 4096 },
      { id: 2, file_name: 'notes.txt', media_type: 'text/plain', size: 21 },
    ],
    remote_image_count: 1,
    ...over,
  };
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.settings.mockReset();
  ipc.settings.mockResolvedValue({
    settings: {
      theme: 'light',
      twenty_four_clock: true,
      zone: 'Etc/UTC',
      first_day_of_week: 'mo',
      horizon_days: 42,
      quote_below: true,
      view_mode: 'flat',
    },
    accounts: [],
    about: { version: 'test' },
  });
  ipc.reading.mockReset();
  ipc.reading.mockResolvedValue(readingSnapshot());
  currentWindow.close.mockReset();
  currentWindow.close.mockResolvedValue(undefined);
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useAppearanceStore.setState({ readingZoom: 'default' });
  window.print = vi.fn();
});

afterEach(() => {
  cleanup();
  delete (window as { print?: unknown }).print;
});

/** Flushes the hydration's pull chain. */
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe('the print document', () => {
  it('renders the sanitised reading view: subject, header lines, body, attachment names', async () => {
    render(<PrintWindow message={message} />);
    await settle();
    await Promise.resolve();

    expect(screen.getByRole('heading', { name: 'Launch' })).toBeTruthy();
    expect(screen.getByText('Ada Lovelace')).toBeTruthy();
    expect(screen.getByText('ada@example.test')).toBeTruthy();
    expect(screen.getByText('9 Oct 2026, 08:30')).toBeTruthy();
    // The sanitised body rides the reading pane's own instrument: a
    // scriptless sandboxed frame over the wrapped document with the gated
    // CSP on it, and the remote placeholder stays a placeholder inside.
    const body = screen.getByTestId('print-body');
    expect(body.getAttribute('sandbox')).toBe('allow-same-origin');
    expect(body.getAttribute('csp')).toBe(READING_CSP_GATED);
    expect(body.getAttribute('srcdoc')).toBe(wrapBodyDocument(sanitisedHtml, 'default'));
    expect(screen.getByText('report.pdf')).toBeTruthy();
    expect(screen.getByText('notes.txt')).toBeTruthy();
  });

  it('scales the printed body by the standing zoom step', async () => {
    useAppearanceStore.setState({ readingZoom: 'large' });
    render(<PrintWindow message={message} />);
    await settle();
    await Promise.resolve();

    // The zoom rides the wrapper document's base font, so the printed
    // body scales exactly as the pane's does.
    expect(screen.getByTestId('print-body').getAttribute('srcdoc')).toBe(
      wrapBodyDocument(sanitisedHtml, 'large'),
    );
  });

  it('prints itself once the document lands and closes after the dialog settles', async () => {
    render(<PrintWindow message={message} />);
    await settle();
    await Promise.resolve();

    expect(window.print).toHaveBeenCalledTimes(1);

    fireEvent(window, new Event('afterprint'));
    await Promise.resolve();

    expect(currentWindow.close).toHaveBeenCalledTimes(1);
  });

  it('opens its message through the shared open path before pulling', async () => {
    render(<PrintWindow message={message} />);
    await settle();

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"open_message":{"message":{"account":"work","key":"imap:v1:u42"}}}',
    );
  });
});
