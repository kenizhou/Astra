// SPDX-License-Identifier: GPL-3.0-only
// The share flow's wire half: the subscription the composer opens from
// (the host's share channel, the mailto sibling), and the route a landing
// takes under Tauri, a titled window on a minted composition carrying the
// files as the pending share set. The path rides that pending state and
// never renders, never logs.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const event = vi.hoisted(() => ({ listen: vi.fn() }));
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

vi.mock('@tauri-apps/api/event', () => ({ listen: event.listen }));
vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: webviewWindow.MockWebviewWindow,
}));
vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

import { useAccountStore } from '@/stores/accountStore';

import { useComposerStore } from './composerStore';
import { onShareOpen, openShareComposer } from './shareOpen';

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

const files = [{ name: 'notes.txt', path: 'C:\\Users\\ada\\notes.txt' }];

beforeEach(() => {
  event.listen.mockReset();
  webviewWindow.instances.length = 0;
  useAccountStore.setState({
    accounts: [{ accountId: 'work', displayName: 'Work', address: 'me@work.example' }],
  });
  useComposerStore.getState().closePane();
});

describe('the share subscription', () => {
  it('subscribes to the share channel and hands the payload over', async () => {
    const unlisten = vi.fn();
    event.listen.mockResolvedValue(unlisten);
    const seen: unknown[] = [];

    const teardown = await onShareOpen((payload: { name: string; path: string }[]) => {
      seen.push(payload);
    });

    const [name, handler] = event.listen.mock.calls[0] as [
      string,
      (event: { payload: unknown }) => void,
    ];
    expect(name).toBe('share_open');
    handler({ payload: files });
    expect(seen).toEqual([files]);

    teardown();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});

describe('the share route under Tauri', () => {
  it('opens a window titled by the file and carrying the pending set', async () => {
    await openShareComposer(files);

    expect(webviewWindow.instances).toHaveLength(1);
    const [window] = webviewWindow.instances;
    expect(window?.label).toMatch(/^composer-/);
    expect(window?.label.replace('composer-', '')).toMatch(UUID);
    const composition = window?.label.replace('composer-', '') ?? '';
    expect(window?.options).toMatchObject({
      url: `index.html?composer=${composition}&share=${encodeURIComponent(JSON.stringify(files))}`,
      title: 'notes.txt',
    });
    expect(useComposerStore.getState().open).toBe(false);
  });

  it('titles a many-file landing New message', async () => {
    await openShareComposer([
      { name: 'notes.txt', path: 'C:\\a.txt' },
      { name: 'photo.png', path: 'C:\\b.png' },
    ]);

    const [window] = webviewWindow.instances;
    expect(window?.options).toMatchObject({ title: 'New message' });
  });

  it('opens nothing without an account', async () => {
    useAccountStore.setState({ accounts: [] });

    await openShareComposer(files);

    expect(webviewWindow.instances).toHaveLength(0);
  });

  it('opens nothing for an empty landing', async () => {
    await openShareComposer([]);

    expect(webviewWindow.instances).toHaveLength(0);
  });
});
