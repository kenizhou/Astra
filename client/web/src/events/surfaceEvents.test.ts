// SPDX-License-Identifier: GPL-3.0-only
// The surface_changed wiring: a change on one surface pulls that surface's
// snapshot into its store, hydration seeds every surface, and the listener
// registry disposes safely under a StrictMode double mount.
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: surfaceEvents registers listeners only inside Tauri,
  // so the jsdom test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const wire = vi.hoisted(() => ({
  handlers: [] as Array<(event: { payload: unknown }) => void>,
  unlistens: 0,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_event: string, handler: (event: { payload: unknown }) => void) => {
    wire.handlers.push(handler);
    return () => {
      wire.unlistens += 1;
    };
  }),
}));

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  mailboxList: vi.fn(),
  syncProgress: vi.fn(),
  reading: vi.fn(),
  draftStatus: vi.fn(),
  addAccount: vi.fn(),
  imapTestConnection: vi.fn(),
}));

vi.mock('@/services/ipc', () => ipc);

import { hydrate, mountSurfaceEvents } from './surfaceEvents';
import { useAccountStore } from '@/stores/accountStore';
import { useDraftStore } from '@/stores/draftStore';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { useUiStore } from '@/stores/uiStore';
import mailboxFixture from '@/services/__fixtures__/mailbox_list.json';

/** The open message's reading snapshot the reading pull resolves with. */
const readingSnapshot = {
  message: { account: 'work', key: 'imap:v1:u42' },
  subject: 'Quarterly report',
  from_name: 'Ada Lovelace',
  from_address: 'ada@example.test',
  sent_at: '2026-10-09T08:30:00Z',
  remote_image_count: 0,
};

function emit(surface: string) {
  for (const handler of wire.handlers) {
    handler({ payload: { surface } });
  }
}

function resetStores() {
  useFolderStore.setState({ accounts: [], selected: null });
  useAccountStore.setState({ accounts: [] });
  useListStore.setState({
    rows: [],
    viewMode: 'flat',
    visibleLimit: 0,
    selection: null,
    selectedKey: null,
  });
  useUiStore.setState({
    engineState: 'starting',
    sync: [],
    accountSetupOpen: false,
  });
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useDraftStore.setState({ snapshot: null });
}

beforeEach(() => {
  wire.handlers.length = 0;
  wire.unlistens = 0;
  ipc.mailboxList.mockReset();
  ipc.syncProgress.mockReset();
  ipc.reading.mockReset();
  ipc.reading.mockResolvedValue(null);
  ipc.draftStatus.mockReset();
  ipc.draftStatus.mockResolvedValue(null);
  resetStores();
});

describe('the surface_changed listener', () => {
  it('a mailbox_list change pulls the snapshot into the folder, account, and list stores', async () => {
    ipc.mailboxList.mockResolvedValue(mailboxFixture);
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('mailbox_list');

    await vi.waitFor(() => expect(useFolderStore.getState().accounts).toHaveLength(2));
    expect(useAccountStore.getState().accounts.map((a) => a.displayName)).toEqual([
      'work@example.net',
      'Home',
    ]);
    expect(useFolderStore.getState().accounts[0]?.folders).toHaveLength(4);
    expect(useListStore.getState().rows.map((r) => r.subject)).toEqual([
      'Quarterly report',
      'Your weekly digest',
    ]);
    expect(useListStore.getState().visibleLimit).toBe(50);
    expect(ipc.syncProgress).not.toHaveBeenCalled();
  });

  it('a sync_progress change pulls the snapshot into the ui store', async () => {
    ipc.syncProgress.mockResolvedValue({
      accounts: [{ account: 'work', state: 'syncing', detail: 'folders 3 of 10' }],
    });
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('sync_progress');

    await vi.waitFor(() => expect(useUiStore.getState().sync).toHaveLength(1));
    expect(useUiStore.getState().sync[0]).toEqual({
      account: 'work',
      state: 'syncing',
      detail: 'folders 3 of 10',
    });
    expect(ipc.mailboxList).not.toHaveBeenCalled();
  });

  it('a reading change pulls the snapshot into the reading store', async () => {
    ipc.reading.mockResolvedValue(readingSnapshot);
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('reading');

    await vi.waitFor(() =>
      expect(useReadingStore.getState().snapshot?.message.key).toBe('imap:v1:u42'),
    );
    expect(useReadingStore.getState().snapshot?.subject).toBe('Quarterly report');
    expect(ipc.mailboxList).not.toHaveBeenCalled();
    expect(ipc.syncProgress).not.toHaveBeenCalled();
  });

  it('a reading change with nothing open clears the store', async () => {
    useReadingStore.setState({
      snapshot: readingSnapshot,
      showRemoteImages: true,
    });
    ipc.reading.mockResolvedValue(null);
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('reading');

    await vi.waitFor(() => expect(useReadingStore.getState().snapshot).toBeNull());
    expect(useReadingStore.getState().showRemoteImages).toBe(false);
  });

  it('a draft_status change pulls the snapshot into the draft store', async () => {
    ipc.draftStatus.mockResolvedValue({
      composition: 'compose-1',
      state: 'saved',
      saved_at: '2026-10-09T08:30:00Z',
    });
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('draft_status');

    await vi.waitFor(() =>
      expect(useDraftStore.getState().snapshot?.composition).toBe('compose-1'),
    );
    expect(useDraftStore.getState().snapshot?.state).toBe('saved');
    expect(ipc.mailboxList).not.toHaveBeenCalled();
  });

  it('a draft_status change with nothing saved clears the store', async () => {
    useDraftStore.setState({
      snapshot: { composition: 'compose-1', state: 'saved', saved_at: '2026-10-09T08:30:00Z' },
    });
    ipc.draftStatus.mockResolvedValue(null);
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('draft_status');

    await vi.waitFor(() => expect(useDraftStore.getState().snapshot).toBeNull());
  });

  it('an unknown surface name is ignored without a pull', async () => {
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('something_else');

    expect(ipc.mailboxList).not.toHaveBeenCalled();
    expect(ipc.syncProgress).not.toHaveBeenCalled();
    expect(ipc.reading).not.toHaveBeenCalled();
    expect(ipc.draftStatus).not.toHaveBeenCalled();
  });

  it('disposing before the registration resolves still tears it down', async () => {
    const dispose = mountSurfaceEvents();
    dispose();
    // The listen promise resolved only now, after disposeAll: the late
    // registration must unlisten itself rather than leak a live handler.
    await vi.waitFor(() => expect(wire.unlistens).toBe(1));
  });
});

describe('hydration', () => {
  it('pulls every surface, marks the engine running, and opens setup when there is no account', async () => {
    ipc.mailboxList.mockResolvedValue({ account_folders: [] });
    ipc.syncProgress.mockResolvedValue({ accounts: [] });

    await hydrate();

    expect(ipc.mailboxList).toHaveBeenCalledTimes(1);
    expect(ipc.syncProgress).toHaveBeenCalledTimes(1);
    expect(ipc.reading).toHaveBeenCalledTimes(1);
    expect(ipc.draftStatus).toHaveBeenCalledTimes(1);
    expect(useReadingStore.getState().snapshot).toBeNull();
    expect(useDraftStore.getState().snapshot).toBeNull();
    expect(useUiStore.getState().engineState).toBe('running');
    expect(useUiStore.getState().accountSetupOpen).toBe(true);
  });

  it('leaves setup closed when an account already exists', async () => {
    ipc.mailboxList.mockResolvedValue(mailboxFixture);
    ipc.syncProgress.mockResolvedValue({ accounts: [] });

    await hydrate();

    expect(useAccountStore.getState().accounts).toHaveLength(2);
    expect(useUiStore.getState().accountSetupOpen).toBe(false);
  });

  it('seeds an open reading the host published before the mount', async () => {
    ipc.mailboxList.mockResolvedValue(mailboxFixture);
    ipc.syncProgress.mockResolvedValue({ accounts: [] });
    ipc.reading.mockResolvedValue(readingSnapshot);

    await hydrate();

    expect(useReadingStore.getState().snapshot?.message.key).toBe('imap:v1:u42');
  });
});
