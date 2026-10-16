// SPDX-License-Identifier: GPL-3.0-only
// The pop-out composer window's mechanics: the query param the main window
// reads the composition id from, the URL and label the window opens with
// (the id alone crossing, the capability wildcard matching the label), and
// the New message route opening a window rather than touching the inline
// composer while under Tauri.
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

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

import { useAccountStore } from '@/stores/accountStore';

import { useComposerStore } from './composerStore';
import {
  composerWindowLabel,
  composerWindowUrl,
  openComposerWindow,
  openNewMessageComposer,
  readComposerParam,
  readResumeParam,
} from './composeWindow';

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

beforeEach(() => {
  webviewWindow.instances.length = 0;
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useComposerStore.getState().closePane();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('the composer query param', () => {
  it('reads the composition id the URL carries', () => {
    expect(readComposerParam('?composer=compose-9')).toBe('compose-9');
    expect(readComposerParam('?composer=compose-9&other=1')).toBe('compose-9');
  });

  it('reads nothing for the main window', () => {
    expect(readComposerParam('')).toBeNull();
    expect(readComposerParam('?other=1')).toBeNull();
    expect(readComposerParam('?composer=')).toBeNull();
  });
});

describe('the resume query param', () => {
  it('reads the account a transfer window resumes', () => {
    expect(readResumeParam('?composer=compose-9&resume=work')).toBe('work');
  });

  it('reads nothing for a plain pop-out', () => {
    expect(readResumeParam('')).toBeNull();
    expect(readResumeParam('?composer=compose-9')).toBeNull();
    expect(readResumeParam('?composer=compose-9&resume=')).toBeNull();
  });
});

describe('the window address', () => {
  it('carries the composition id alone in the URL', () => {
    expect(composerWindowUrl('compose-9')).toBe('index.html?composer=compose-9');
  });

  it('marks the resume account on a transfer', () => {
    expect(composerWindowUrl('compose-9', 'work')).toBe(
      'index.html?composer=compose-9&resume=work',
    );
  });

  it('labels the window under the composer capability wildcard', () => {
    expect(composerWindowLabel('compose-9')).toBe('composer-compose-9');
  });
});

describe('the window opening', () => {
  it('opens a frameless centred window on the composition', async () => {
    await openComposerWindow('compose-9', 'Re: Quarterly report');

    expect(webviewWindow.instances).toHaveLength(1);
    const [window] = webviewWindow.instances;
    expect(window?.label).toBe('composer-compose-9');
    expect(window?.options).toMatchObject({
      url: 'index.html?composer=compose-9',
      title: 'Re: Quarterly report',
      minWidth: 600,
      minHeight: 480,
      center: true,
      decorations: false,
      resizable: true,
    });
    expect(window?.once).toHaveBeenCalledWith('tauri://error', expect.any(Function));
  });

  it('forwards the resume account onto the URL for a transfer', async () => {
    await openComposerWindow('compose-9', 'Re: Quarterly report', 'work');

    const [window] = webviewWindow.instances;
    expect(window?.options).toMatchObject({
      url: 'index.html?composer=compose-9&resume=work',
    });
  });
});

describe('the New message route', () => {
  it('opens a window on a minted composition and leaves the inline composer shut', async () => {
    await openNewMessageComposer();

    expect(webviewWindow.instances).toHaveLength(1);
    const [window] = webviewWindow.instances;
    expect(window?.label).toMatch(/^composer-/);
    expect(window?.label.replace('composer-', '')).toMatch(UUID);
    expect(window?.options).toMatchObject({ title: 'New message' });
    expect(useComposerStore.getState().open).toBe(false);
  });

  it('opens nothing without an account', async () => {
    useAccountStore.setState({ accounts: [] });

    await openNewMessageComposer();

    expect(webviewWindow.instances).toHaveLength(0);
  });
});
