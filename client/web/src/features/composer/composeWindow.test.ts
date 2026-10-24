// SPDX-License-Identifier: GPL-3.0-only
// The pop-out composer window's mechanics: the query params the main
// window reads (the composition id, the transfer's resume account, a
// mailto window's prefill), the URL and label the window opens with
// (the id and, for a mailto window, the host-parsed prefill crossing,
// the capability wildcard matching the label), and the New message and
// mailto routes opening a window rather than touching the inline
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
import { useSettingsStore } from '@/features/settings/settingsStore';

import { useComposerStore, type MailtoOpen } from './composerStore';
import { openMailtoComposer } from './mailtoOpen';
import {
  composerWindowLabel,
  composerWindowUrl,
  defaultSendAccount,
  openComposerWindow,
  openNewMessageComposer,
  readComposerParam,
  readPrefillParam,
  readResumeParam,
  readShareParam,
} from './composeWindow';

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** The prefill a mailto link opens, as the host's event carries it. */
const prefill: MailtoOpen = {
  from: 'work',
  to: 'ada@example.test',
  cc: 'tim@example.test',
  bcc: '',
  subject: 'Lunch',
  body: 'Noon?',
};

/** The encoded prefill param the URL carries. */
const encodedPrefill = encodeURIComponent(JSON.stringify(prefill));

beforeEach(() => {
  webviewWindow.instances.length = 0;
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work', address: 'me@work.example' }] });
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

describe('the prefill query param', () => {
  it('reads the prefill the URL carries', () => {
    expect(readPrefillParam(`?composer=compose-9&prefill=${encodedPrefill}`)).toEqual(prefill);
  });

  it('reads nothing for a plain pop-out', () => {
    expect(readPrefillParam('?composer=compose-9')).toBeNull();
    expect(readPrefillParam('?composer=compose-9&prefill=')).toBeNull();
  });

  it('refuses a prefill that is not the wire shape', () => {
    expect(
      readPrefillParam(`?composer=compose-9&prefill=${encodeURIComponent('not json')}`),
    ).toBeNull();
    expect(
      readPrefillParam(
        `?composer=compose-9&prefill=${encodeURIComponent(JSON.stringify({ from: 'work' }))}`,
      ),
    ).toBeNull();
  });
});

describe('the share query param', () => {
  const shares = [{ name: 'notes.txt', path: 'C:\\Users\\ada\\notes.txt' }];
  const encodedShares = encodeURIComponent(JSON.stringify(shares));

  it('reads the pending share set the URL carries', () => {
    expect(readShareParam(`?composer=compose-9&share=${encodedShares}`)).toEqual(shares);
  });

  it('reads nothing for a plain pop-out', () => {
    expect(readShareParam('?composer=compose-9')).toBeNull();
    expect(readShareParam('?composer=compose-9&share=')).toBeNull();
  });

  it('refuses a share set that is not the wire shape', () => {
    expect(
      readShareParam(`?composer=compose-9&share=${encodeURIComponent('not json')}`),
    ).toBeNull();
    expect(
      readShareParam(
        `?composer=compose-9&share=${encodeURIComponent(JSON.stringify([{ name: 'x' }]))}`,
      ),
    ).toBeNull();
    expect(
      readShareParam(
        `?composer=compose-9&share=${encodeURIComponent(JSON.stringify([{ path: 'C:\\x' }]))}`,
      ),
    ).toBeNull();
  });

  it('rides the URL after the prefill when both cross', () => {
    expect(composerWindowUrl('compose-9', undefined, prefill, shares)).toBe(
      `index.html?composer=compose-9&prefill=${encodedPrefill}&share=${encodedShares}`,
    );
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

  it('carries the prefill a mailto window opens from', () => {
    expect(composerWindowUrl('compose-9', undefined, prefill)).toBe(
      `index.html?composer=compose-9&prefill=${encodedPrefill}`,
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

  it('carries the prefill onto the URL for a mailto window', async () => {
    await openComposerWindow('compose-9', 'Lunch', undefined, prefill);

    const [window] = webviewWindow.instances;
    expect(window?.options).toMatchObject({
      url: `index.html?composer=compose-9&prefill=${encodedPrefill}`,
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

  it('resolves the default send account when the preference names a standing one', () => {
    useAccountStore.setState({
      accounts: [
        { accountId: 'work', displayName: 'Work', address: 'me@work.example' },
        { accountId: 'home', displayName: 'Home', address: 'me@home.example' },
      ],
    });
    useSettingsStore.setState({
      settings: { ...useSettingsStore.getState().settings, default_send_account: 'home' },
    });

    expect(defaultSendAccount()).toEqual({ id: 'home', label: 'Home' });
  });

  it('falls back to the first account when the preference stands unset or stale', () => {
    useAccountStore.setState({
      accounts: [
        { accountId: 'work', displayName: 'Work', address: 'me@work.example' },
        { accountId: 'home', displayName: 'Home', address: 'me@home.example' },
      ],
    });
    useSettingsStore.setState({
      settings: { ...useSettingsStore.getState().settings, default_send_account: 'gone' },
    });

    expect(defaultSendAccount()).toEqual({ id: 'work', label: 'Work' });
  });
});

describe('the mailto route', () => {
  it('opens a prefilled window titled by the subject on a minted composition', async () => {
    await openMailtoComposer(prefill);

    expect(webviewWindow.instances).toHaveLength(1);
    const [window] = webviewWindow.instances;
    expect(window?.label).toMatch(/^composer-/);
    expect(window?.label.replace('composer-', '')).toMatch(UUID);
    expect(window?.options).toMatchObject({
      url: `index.html?composer=${window?.label.replace('composer-', '')}&prefill=${encodedPrefill}`,
      title: 'Lunch',
    });
    expect(useComposerStore.getState().open).toBe(false);
  });

  it('titles a subjectless prefill New message', async () => {
    await openMailtoComposer({ ...prefill, subject: '' });

    const [window] = webviewWindow.instances;
    expect(window?.options).toMatchObject({ title: 'New message' });
  });

  it('opens a second window for a second mailto while one stands, each on its own composition', async () => {
    await openMailtoComposer(prefill);
    await openMailtoComposer({ ...prefill, subject: 'Again' });

    expect(webviewWindow.instances).toHaveLength(2);
    const [first, second] = webviewWindow.instances;
    expect(first?.label).not.toBe(second?.label);
    expect(useComposerStore.getState().open).toBe(false);
  });
});
