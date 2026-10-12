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
  composeRequest: vi.fn(),
  sending: vi.fn(),
  calendar: vi.fn(),
  people: vi.fn(),
  settings: vi.fn(),
  addAccount: vi.fn(),
  imapTestConnection: vi.fn(),
}));

vi.mock('@/services/ipc', () => ipc);

const ai = vi.hoisted(() => ({ splitMap: vi.fn() }));

vi.mock('@/services/ai', () => ai);

import { mountSurfaceEvents } from './surfaceEvents';
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
import type { ComposeRequestSnapshot } from '@/services/ipc';
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

/** The reply's prefill the compose-request pull resolves with. */
const composeRequestSnapshot: ComposeRequestSnapshot = {
  kind: 'reply',
  from: 'work',
  to: 'ada@example.test',
  cc: '',
  subject: 'Re: Quarterly report',
  in_reply_to: 'orig-1@server.test',
  document: { blocks: [], attachments: [] },
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
  wire.handlers.length = 0;
  wire.unlistens = 0;
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
  ipc.settings.mockReset();
  ipc.settings.mockResolvedValue({});
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

  it('a mailbox_list change refreshes the split map beside the snapshot', async () => {
    ipc.mailboxList.mockResolvedValue(mailboxFixture);
    ai.splitMap.mockImplementation(async (account: string) =>
      account === 'work' ? [{ key: 'jmap:e2', side: 'fyi' }] : [],
    );
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('mailbox_list');

    await vi.waitFor(() => expect(ai.splitMap).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(useSplitStore.getState().entries['work']).toBeDefined());
    expect(useSplitStore.getState().entries['work']?.get('jmap:e2')).toBe('fyi');
    expect(useSplitStore.getState().entries['home']?.size).toBe(0);
  });

  it('a rejecting split map never blocks the snapshot or the other accounts', async () => {
    ipc.mailboxList.mockResolvedValue(mailboxFixture);
    ai.splitMap.mockImplementation(async (account: string) => {
      if (account === 'work') {
        throw new Error('the split read refused');
      }
      return [{ key: 'imap:v1:u9', side: 'actioned' }];
    });
    const errorLog = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('mailbox_list');

    // The snapshot itself landed, the healthy account's map applied, and
    // the refusing account's absence is the only trace of its failure.
    await vi.waitFor(() => expect(useFolderStore.getState().accounts).toHaveLength(2));
    await vi.waitFor(() =>
      expect(useSplitStore.getState().entries['home']?.get('imap:v1:u9')).toBe('actioned'),
    );
    expect(useSplitStore.getState().entries['work']).toBeUndefined();
    expect(errorLog).toHaveBeenCalled();
    errorLog.mockRestore();
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

  it('a compose_request change pulls the snapshot into the compose request store', async () => {
    ipc.composeRequest.mockResolvedValue(composeRequestSnapshot);
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('compose_request');

    await vi.waitFor(() =>
      expect(useComposeRequestStore.getState().snapshot?.subject).toBe('Re: Quarterly report'),
    );
    expect(useComposeRequestStore.getState().snapshot?.kind).toBe('reply');
    expect(useComposeRequestStore.getState().snapshot?.in_reply_to).toBe('orig-1@server.test');
    expect(ipc.mailboxList).not.toHaveBeenCalled();
    expect(ipc.draftStatus).not.toHaveBeenCalled();
  });

  it('a compose_request change with nothing pending clears the store', async () => {
    useComposeRequestStore.setState({ snapshot: composeRequestSnapshot });
    ipc.composeRequest.mockResolvedValue(null);
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('compose_request');

    await vi.waitFor(() =>
      expect(useComposeRequestStore.getState().snapshot).toBeNull(),
    );
  });

  it('a sending change pulls the snapshot into the sending store', async () => {
    ipc.sending.mockResolvedValue({
      rows: [
        {
          queued: { account: 'work', op: 3 },
          state: 'queued',
          subject: 'Quarterly review',
          arrived_at: '2026-10-09T08:30:00Z',
        },
      ],
    });
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('sending');

    await vi.waitFor(() => expect(useSendingStore.getState().snapshot.rows).toHaveLength(1));
    expect(useSendingStore.getState().snapshot.rows[0]).toEqual({
      queued: { account: 'work', op: 3 },
      state: 'queued',
      subject: 'Quarterly review',
      arrived_at: '2026-10-09T08:30:00Z',
    });
    expect(ipc.mailboxList).not.toHaveBeenCalled();
    expect(ipc.draftStatus).not.toHaveBeenCalled();
  });

  it('a sending change with nothing outstanding empties the store', async () => {
    useSendingStore.setState({
      snapshot: {
        rows: [
          {
            queued: { account: 'work', op: 3 },
            state: 'sending',
            subject: 'Quarterly review',
            arrived_at: '2026-10-09T08:30:00Z',
          },
        ],
      },
    });
    ipc.sending.mockResolvedValue({ rows: [] });
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('sending');

    await vi.waitFor(() => expect(useSendingStore.getState().snapshot.rows).toHaveLength(0));
  });

  it('a calendar change pulls the snapshot into the calendar store', async () => {
    ipc.calendar.mockResolvedValue({
      calendars: [
        {
          account: 'work',
          id: 'cal-1',
          name: 'Work',
          visible: true,
          can_write: true,
          is_default: true,
        },
      ],
      occurrences: [],
      now_data: { now: '2026-10-19T22:30:00Z', zone: 'Europe/Amsterdam' },
    });
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('calendar');

    await vi.waitFor(() => expect(useCalendarStore.getState().calendars).toHaveLength(1));
    expect(useCalendarStore.getState().anchor).toBe('2026-10-20');
    expect(ipc.mailboxList).not.toHaveBeenCalled();
  });

  it('a people change pulls the snapshot into the people store', async () => {
    ipc.people.mockResolvedValue({
      rows: [
        {
          person: 7,
          display_name: 'Ada Lovelace',
          monogram: { initials: 'AL', colour: '#4DB6AC' },
          account_count: 2,
          section: 'A',
          primary_address: 'ada@example.test',
        },
      ],
      write_states: [{ account: 'work', state: 'writing' }],
    });
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('people');

    await vi.waitFor(() => expect(usePeopleStore.getState().rows).toHaveLength(1));
    expect(usePeopleStore.getState().rows[0]?.display_name).toBe('Ada Lovelace');
    expect(usePeopleStore.getState().writeStates).toEqual([{ account: 'work', state: 'writing' }]);
    expect(ipc.mailboxList).not.toHaveBeenCalled();
    expect(ipc.calendar).not.toHaveBeenCalled();
  });

  it('a settings change pulls the settings snapshot', async () => {
    ipc.settings.mockResolvedValue({
      settings: {
        theme: 'dark',
        twenty_four_clock: true,
        zone: 'Europe/Amsterdam',
        first_day_of_week: 'su',
        horizon_days: 31,
        quote_below: false,
        view_mode: 'threaded',
      },
      about: { version: '0.1.0' },
    });
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('settings');

    await vi.waitFor(() => expect(ipc.settings).toHaveBeenCalledTimes(1));
    expect(ipc.mailboxList).not.toHaveBeenCalled();
    expect(ipc.calendar).not.toHaveBeenCalled();
  });

  it('an unknown surface name is ignored without a pull', async () => {
    mountSurfaceEvents();
    await vi.waitFor(() => expect(wire.handlers).toHaveLength(1));

    emit('something_else');

    expect(ipc.mailboxList).not.toHaveBeenCalled();
    expect(ipc.syncProgress).not.toHaveBeenCalled();
    expect(ipc.reading).not.toHaveBeenCalled();
    expect(ipc.draftStatus).not.toHaveBeenCalled();
    expect(ipc.composeRequest).not.toHaveBeenCalled();
    expect(ipc.sending).not.toHaveBeenCalled();
    expect(ipc.settings).not.toHaveBeenCalled();
  });

  it('disposing before the registration resolves still tears it down', async () => {
    const dispose = mountSurfaceEvents();
    dispose();
    // The listen promise resolved only now, after disposeAll: the late
    // registration must unlisten itself rather than leak a live handler.
    await vi.waitFor(() => expect(wire.unlistens).toBe(1));
  });
});
