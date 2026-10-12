// SPDX-License-Identifier: GPL-3.0-only
// Hydration: the first pull at mount draws every surface, including the
// people snapshot, marks the engine running once they land, and opens the
// account setup flow when there is no account to show; the split map rides
// the list pull and a refusing read never blocks the rest.
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
  // The Tauri sentinel: surfaceEvents pulls snapshots only inside Tauri, so
  // the jsdom test stands in as one.
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = {};
});

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  mailboxList: vi.fn(),
  syncProgress: vi.fn(),
  reading: vi.fn(),
  draftStatus: vi.fn(),
  composeRequest: vi.fn(),
  sending: vi.fn(),
  calendar: vi.fn(),
  people: vi.fn(),
  addAccount: vi.fn(),
  imapTestConnection: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => undefined),
}));

vi.mock('@/services/ipc', () => ipc);

const ai = vi.hoisted(() => ({ splitMap: vi.fn() }));

vi.mock('@/services/ai', () => ai);

import { hydrate } from './surfaceEvents';
import { useAccountStore } from '@/stores/accountStore';
import { useCalendarStore } from '@/features/calendar/calendarStore';
import { usePeopleStore } from '@/features/people/peopleStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';
import { useDraftStore } from '@/stores/draftStore';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { useSendingStore } from '@/stores/sendingStore';
import { useSplitStore } from '@/stores/splitStore';
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
  useComposeRequestStore.setState({ snapshot: null });
  useSendingStore.setState({ snapshot: { rows: [] } });
  useCalendarStore.setState({
    calendars: [],
    occurrences: [],
    writeStates: [],
    replyPrompts: [],
    nowData: null,
    view: 'month',
    anchor: null,
  });
  usePeopleStore.setState({
    rows: [],
    detail: null,
    writeStates: [],
    createTargets: [],
  });
  useSplitStore.setState({ entries: {}, filter: 'all' });
}

beforeEach(() => {
  ai.splitMap.mockReset();
  ai.splitMap.mockResolvedValue([]);
  ipc.mailboxList.mockReset();
  ipc.syncProgress.mockReset();
  ipc.reading.mockReset();
  ipc.reading.mockResolvedValue(null);
  ipc.draftStatus.mockReset();
  ipc.draftStatus.mockResolvedValue(null);
  ipc.composeRequest.mockReset();
  ipc.composeRequest.mockResolvedValue(null);
  ipc.sending.mockReset();
  ipc.sending.mockResolvedValue({ rows: [] });
  ipc.calendar.mockReset();
  ipc.calendar.mockResolvedValue({});
  ipc.people.mockReset();
  ipc.people.mockResolvedValue({});
  resetStores();
});

describe('hydration', () => {
  it('pulls every surface, marks the engine running, and opens setup when there is no account', async () => {
    ipc.mailboxList.mockResolvedValue({ account_folders: [] });
    ipc.syncProgress.mockResolvedValue({ accounts: [] });
    ipc.calendar.mockResolvedValue({});

    await hydrate();

    expect(ipc.mailboxList).toHaveBeenCalledTimes(1);
    expect(ipc.syncProgress).toHaveBeenCalledTimes(1);
    expect(ipc.reading).toHaveBeenCalledTimes(1);
    expect(ipc.draftStatus).toHaveBeenCalledTimes(1);
    expect(ipc.composeRequest).toHaveBeenCalledTimes(1);
    expect(ipc.sending).toHaveBeenCalledTimes(1);
    expect(ipc.calendar).toHaveBeenCalledTimes(1);
    expect(ipc.people).toHaveBeenCalledTimes(1);
    expect(useReadingStore.getState().snapshot).toBeNull();
    expect(useDraftStore.getState().snapshot).toBeNull();
    expect(useComposeRequestStore.getState().snapshot).toBeNull();
    expect(useSendingStore.getState().snapshot.rows).toHaveLength(0);
    expect(useCalendarStore.getState().calendars).toEqual([]);
    expect(usePeopleStore.getState().rows).toEqual([]);
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

  it('pulls the split map for every account beside the mailbox list', async () => {
    ipc.mailboxList.mockResolvedValue(mailboxFixture);
    ipc.syncProgress.mockResolvedValue({ accounts: [] });
    ai.splitMap.mockImplementation(async (account: string) =>
      account === 'home' ? [{ key: 'imap:v1:u9', side: 'actioned' }] : [],
    );

    await hydrate();

    expect(ai.splitMap).toHaveBeenCalledWith('work');
    expect(ai.splitMap).toHaveBeenCalledWith('home');
    expect(useSplitStore.getState().entries['home']?.get('imap:v1:u9')).toBe('actioned');
  });

  it('continues hydration when a split map rejects', async () => {
    ipc.mailboxList.mockResolvedValue(mailboxFixture);
    ipc.syncProgress.mockResolvedValue({ accounts: [] });
    ai.splitMap.mockRejectedValue(new Error('the split read refused'));
    const errorLog = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    await hydrate();

    // The mail surfaces hydrated and the engine reached running despite
    // the split read's refusal.
    expect(useListStore.getState().rows).toHaveLength(2);
    expect(useUiStore.getState().engineState).toBe('running');
    expect(useUiStore.getState().accountSetupOpen).toBe(false);
    errorLog.mockRestore();
  });

  it('seeds an open reading the host published before the mount', async () => {
    ipc.mailboxList.mockResolvedValue(mailboxFixture);
    ipc.syncProgress.mockResolvedValue({ accounts: [] });
    ipc.reading.mockResolvedValue(readingSnapshot);

    await hydrate();

    expect(useReadingStore.getState().snapshot?.message.key).toBe('imap:v1:u42');
  });

  it('seeds the people snapshot a host published before the mount', async () => {
    ipc.mailboxList.mockResolvedValue({ account_folders: [] });
    ipc.syncProgress.mockResolvedValue({ accounts: [] });
    ipc.people.mockResolvedValue({
      rows: [
        {
          person: 7,
          display_name: 'Ada Lovelace',
          monogram: { initials: 'AL', colour: '#4DB6AC' },
          section: 'A',
          primary_address: 'ada@example.test',
        },
      ],
    });

    await hydrate();

    expect(usePeopleStore.getState().rows[0]?.display_name).toBe('Ada Lovelace');
  });
});
