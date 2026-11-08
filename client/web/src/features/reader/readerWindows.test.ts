// SPDX-License-Identifier: GPL-3.0-only
// The reading window's mechanics: the query params the main window reads
// (the message's account and key as their own params, the folder the list
// sat in, the print marker), the URLs and labels the windows open with
// (the message-* capability wildcard matching both), and the two opening
// routes: the own window under Tauri, the pane fallback outside it.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const webviewWindow = vi.hoisted(() => {
  interface Instance {
    label: string;
    options: Record<string, unknown>;
    once: ReturnType<typeof vi.fn>;
  }
  const instances: Instance[] = [];
  class MockWebviewWindow {
    once = vi.fn().mockResolvedValue(() => undefined);
    constructor(
      public label: string,
      public options: Record<string, unknown>,
    ) {
      instances.push({ label, options, once: this.once });
    }
  }
  return { MockWebviewWindow, instances };
});

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: webviewWindow.MockWebviewWindow,
}));

const surface = vi.hoisted(() => ({ tauri: true }));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  get isTauri() {
    return surface.tauri;
  },
}));

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', () => ({
  dispatch: ipc.dispatch,
  intents: {
    openMessage: (message: unknown) => ({ open_message: { message } }),
  },
}));

import {
  messageWindowLabel,
  messageWindowUrl,
  openMessageWindow,
  openPrintWindow,
  printWindowLabel,
  printWindowUrl,
  readMessageWindowParams,
} from './readerWindows';

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

beforeEach(() => {
  webviewWindow.instances.length = 0;
  surface.tauri = true;
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('the message query params', () => {
  it('reads the account and key as their own params, each url-decoded', () => {
    const params = readMessageWindowParams(
      '?message-account=work&message-key=imap%3Av1%3Au42',
    );
    expect(params).toEqual({
      message: { account: 'work', key: 'imap:v1:u42' },
      folderKey: null,
      print: false,
    });
  });

  it('reads the folder the list sat in and the print marker', () => {
    const params = readMessageWindowParams(
      '?message-account=work&message-key=m1&folder=INBOX&print=1',
    );
    expect(params).toEqual({
      message: { account: 'work', key: 'm1' },
      folderKey: 'INBOX',
      print: true,
    });
  });

  it('reads nothing for the main window or a half-carried identity', () => {
    expect(readMessageWindowParams('')).toBeNull();
    expect(readMessageWindowParams('?composer=c1')).toBeNull();
    expect(readMessageWindowParams('?message-account=work')).toBeNull();
    expect(readMessageWindowParams('?message-key=m1')).toBeNull();
    expect(readMessageWindowParams('?message-account=&message-key=m1')).toBeNull();
  });
});

describe('the window addresses', () => {
  it('carries the message identity alone in the URL, both halves encoded', () => {
    expect(messageWindowUrl({ account: 'work', key: 'imap:v1:u42' })).toBe(
      'index.html?message-account=work&message-key=imap%3Av1%3Au42',
    );
  });

  it('marks the folder the list sat in when one crossed', () => {
    expect(messageWindowUrl({ account: 'work', key: 'm1' }, 'Junk')).toBe(
      'index.html?message-account=work&message-key=m1&folder=Junk',
    );
  });

  it('marks the print window over the same identity', () => {
    expect(printWindowUrl({ account: 'work', key: 'm1' })).toBe(
      'index.html?message-account=work&message-key=m1&print=1',
    );
  });

  it('labels both windows under the message capability wildcard', () => {
    expect(messageWindowLabel('0f0e0d0c-1b2a-3c4d-5e6f-7a8b9c0d1e2f')).toBe(
      'message-0f0e0d0c-1b2a-3c4d-5e6f-7a8b9c0d1e2f',
    );
    expect(printWindowLabel('0f0e0d0c-1b2a-3c4d-5e6f-7a8b9c0d1e2f')).toBe(
      'message-print-0f0e0d0c-1b2a-3c4d-5e6f-7a8b9c0d1e2f',
    );
  });
});

describe('the own window opening', () => {
  it('opens a frameless centred window on a minted id, titled by the subject', async () => {
    await openMessageWindow(
      { account: 'work', key: 'imap:v1:u42' },
      'Quarterly report',
      'INBOX',
    );

    expect(webviewWindow.instances).toHaveLength(1);
    const [window] = webviewWindow.instances;
    expect(window?.label).toMatch(/^message-/);
    expect(window?.label.replace('message-', '')).toMatch(UUID);
    expect(window?.options).toMatchObject({
      url: `index.html?message-account=work&message-key=imap%3Av1%3Au42&folder=INBOX`,
      title: 'Quarterly report',
      minWidth: 480,
      minHeight: 420,
      center: true,
      decorations: false,
      resizable: true,
    });
    expect(window?.once).toHaveBeenCalledWith('tauri://error', expect.any(Function));
  });

  it('opens without the folder marker when no folder crossed', async () => {
    await openMessageWindow({ account: 'work', key: 'm1' }, 'Launch', null);

    const [window] = webviewWindow.instances;
    expect(window?.options.url).toBe('index.html?message-account=work&message-key=m1');
  });

  it('falls back to opening the message in the pane outside Tauri', async () => {
    surface.tauri = false;

    await openMessageWindow({ account: 'work', key: 'm1' }, 'Launch', null);

    expect(webviewWindow.instances).toHaveLength(0);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      open_message: { message: { account: 'work', key: 'm1' } },
    });
  });
});

describe('the print window opening', () => {
  it('opens a print-labelled window on a minted id over the message', async () => {
    await openPrintWindow({ account: 'work', key: 'm1' });

    expect(webviewWindow.instances).toHaveLength(1);
    const [window] = webviewWindow.instances;
    expect(window?.label).toMatch(/^message-print-/);
    expect(window?.label.replace('message-print-', '')).toMatch(UUID);
    expect(window?.options).toMatchObject({
      url: 'index.html?message-account=work&message-key=m1&print=1',
      title: 'Print',
    });
  });
});
