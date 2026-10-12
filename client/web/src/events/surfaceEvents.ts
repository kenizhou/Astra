// SPDX-License-Identifier: GPL-3.0-only
// The surface_changed wiring: the host emits one event per changed surface and
// the client pulls that surface's snapshot into its store, the loop's second
// half. The mailbox-list pull also refreshes the split inbox's map for every
// account, so the list's toggle always filters against the categorisation
// behind the snapshot's own rows. The listener registry is dispose-safe
// under React StrictMode's double mount: a registration that resolves after
// its effect unmounted unlistens itself instead of leaking a live duplicate
// handler.

import { listen } from '@tauri-apps/api/event';

import { splitMap } from '@/services/ai';
import {
  calendar,
  composeRequest,
  draftStatus,
  mailboxList,
  people,
  reading,
  sending,
  syncProgress,
} from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useCalendarStore } from '@/features/calendar/calendarStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';
import { useDraftStore } from '@/stores/draftStore';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { usePeopleStore } from '@/features/people/peopleStore';
import { useReadingStore } from '@/stores/readingStore';
import { useSendingStore } from '@/stores/sendingStore';
import { useSplitStore } from '@/stores/splitStore';
import { useUiStore } from '@/stores/uiStore';
import type { MailboxListSnapshot } from '@/services/surfaces';

/** The surfaces a change can name, as the event payload carries them. */
export type Surface =
  | 'mailbox_list'
  | 'sync_progress'
  | 'reading'
  | 'draft_status'
  | 'sending'
  | 'compose_request'
  | 'calendar'
  | 'people';

/**
 * Tauri presence guard, evaluated at module load so jsdom tests that install
 * the `__TAURI_INTERNALS__` sentinel before importing this module count as
 * Tauri, and a plain browser dev server stays inert.
 */
export const isTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/**
 * Per-mount listener registry. `safeListen` registers one Tauri event listener
 * in isolation, so one failed registration never silently skips the rest;
 * `disposeAll` unlistens every successful registration.
 */
export interface ListenerRegistry {
  safeListen: <T>(
    event: string,
    handler: (event: { payload: T }) => void,
    label: string,
  ) => Promise<void>;
  disposeAll: () => void;
}

export function createListenerRegistry(): ListenerRegistry {
  const unlisteners: Array<() => void> = [];
  let disposed = false;

  const safeListen = async <T>(
    event: string,
    handler: (event: { payload: T }) => void,
    label: string,
  ): Promise<void> => {
    try {
      const unlisten = await listen<T>(event, handler);
      if (disposed) {
        // The effect unmounted while this listen was in flight; tear the late
        // registration down immediately so it cannot leak a duplicate handler.
        unlisten();
        return;
      }
      unlisteners.push(unlisten);
    } catch (err) {
      console.error(`surface-events: the ${label} listener failed to register`, err);
    }
  };

  const disposeAll = (): void => {
    disposed = true;
    for (const unlisten of unlisteners) {
      try {
        unlisten();
      } catch {
        // One stubborn unlisten never blocks the rest.
      }
    }
    unlisteners.length = 0;
  };

  return { safeListen, disposeAll };
}

/**
 * Pulls every account's split map beside the mailbox-list snapshot: the
 * map is a pure store read the host serves per account, so the list's
 * side views follow every snapshot the same as its rows. Each pull
 * settles alone: one account's refused read logs and steps aside, never
 * aborting the snapshot pull it rides or the hydration behind it.
 */
function pullSplitMaps(snapshot: MailboxListSnapshot): Promise<void> {
  return Promise.all(
    snapshot.account_folders.map((account) =>
      splitMap(account.account_id).then(
        (entries) => {
          useSplitStore.getState().applyMap(account.account_id, entries);
        },
        (err) => {
          console.error(
            `surface-events: the ${account.account_id} split map failed to pull`,
            err,
          );
        },
      ),
    ),
  ).then(() => undefined);
}

/** Pulls one surface's snapshot into the store that renders it. */
export function pullSurface(surface: Surface): Promise<void> {
  if (surface === 'mailbox_list') {
    return mailboxList().then((snapshot) => {
      useAccountStore.getState().applySnapshot(snapshot);
      useFolderStore.getState().applySnapshot(snapshot);
      useListStore.getState().applySnapshot(snapshot);
      return pullSplitMaps(snapshot);
    });
  }
  if (surface === 'reading') {
    return reading().then((snapshot) => {
      useReadingStore.getState().applySnapshot(snapshot);
    });
  }
  if (surface === 'draft_status') {
    return draftStatus().then((snapshot) => {
      useDraftStore.getState().applySnapshot(snapshot);
    });
  }
  if (surface === 'compose_request') {
    return composeRequest().then((snapshot) => {
      useComposeRequestStore.getState().applySnapshot(snapshot);
    });
  }
  if (surface === 'sending') {
    return sending().then((snapshot) => {
      useSendingStore.getState().applySnapshot(snapshot);
    });
  }
  if (surface === 'calendar') {
    return calendar().then((snapshot) => {
      useCalendarStore.getState().applySnapshot(snapshot);
    });
  }
  if (surface === 'people') {
    return people().then((snapshot) => {
      usePeopleStore.getState().applySnapshot(snapshot);
    });
  }
  return syncProgress().then((snapshot) => {
    useUiStore.getState().applySyncProgress(snapshot);
  });
}

/**
 * Mounts the surface_changed listener and returns its disposer. Synchronous on
 * purpose: the caller's effect can unmount before the registration resolves,
 * and the registry handles that race itself.
 */
export function mountSurfaceEvents(): () => void {
  if (!isTauri) {
    return () => undefined;
  }
  const registry = createListenerRegistry();
  void registry.safeListen<{ surface: Surface }>(
    'surface_changed',
    (event) => {
      const { surface } = event.payload;
      if (
        surface === 'mailbox_list' ||
        surface === 'sync_progress' ||
        surface === 'reading' ||
        surface === 'draft_status' ||
        surface === 'sending' ||
        surface === 'compose_request' ||
        surface === 'calendar' ||
        surface === 'people'
      ) {
        void pullSurface(surface);
      }
    },
    'surface_changed',
  );
  return () => registry.disposeAll();
}

/**
 * The first pull at mount: every surface, the engine marked running once they
 * land, and the account setup flow opened when there is no account to show.
 * The reading pull also carries a snapshot the host published before the
 * mount, so the pane and the list highlight start truthful, and the split
 * maps land with the list's first snapshot.
 */
export async function hydrate(): Promise<void> {
  if (!isTauri) {
    return;
  }
  const snapshot = await mailboxList();
  useAccountStore.getState().applySnapshot(snapshot);
  useFolderStore.getState().applySnapshot(snapshot);
  useListStore.getState().applySnapshot(snapshot);
  await pullSplitMaps(snapshot);
  const progress = await syncProgress();
  useUiStore.getState().applySyncProgress(progress);
  const openReading = await reading();
  useReadingStore.getState().applySnapshot(openReading);
  const draftHint = await draftStatus();
  useDraftStore.getState().applySnapshot(draftHint);
  const pendingRequest = await composeRequest();
  useComposeRequestStore.getState().applySnapshot(pendingRequest);
  const queue = await sending();
  useSendingStore.getState().applySnapshot(queue);
  const diary = await calendar();
  useCalendarStore.getState().applySnapshot(diary);
  const directory = await people();
  usePeopleStore.getState().applySnapshot(directory);
  useUiStore.getState().markRunning();
  if (snapshot.account_folders.length === 0) {
    useUiStore.getState().setAccountSetupOpen(true);
  }
}
