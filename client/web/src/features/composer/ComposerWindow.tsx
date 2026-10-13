// SPDX-License-Identifier: GPL-3.0-only
// The pop-out composer window's root: main.tsx mounts it in place of the app
// shell when the URL carries a composition id. It hydrates its own session
// from the host: the accounts and settings pulls (the settings apply the
// theme class), the draft-status pull for the save hint, and the consuming
// take of the pending compose request the reply and forward flows published
// for exactly this window; no pending request means a plain new message on
// the first account. The draft-status surface keeps the status bar's save
// hint live, and the composition leaving the store (a send, a discard, a
// close) closes the OS window.

import { useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { createListenerRegistry, isTauri, pullSurface, type Surface } from '@/events/surfaceEvents';
import {
  draftStatus,
  mailboxList,
  settings,
  takeComposeRequest,
  type CompositionId,
} from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';
import { useDraftStore } from '@/stores/draftStore';
import { useSettingsStore } from '@/features/settings/settingsStore';

import { ComposerPane } from './Composer';
import { ComposerTitleBar } from './ComposerTitleBar';
import { useComposerStore } from './composerStore';
import { fromLabelFor } from './useComposeRequest';

// One hydration per window, StrictMode safe: the double-mounted effect's
// second run finds the session already taken and returns.
let hydratedFor: CompositionId | null = null;

/** Opens the window's composition: the pending request's, or a blank one. */
async function openComposerSession(composition: CompositionId): Promise<void> {
  if (hydratedFor === composition) {
    return;
  }
  hydratedFor = composition;
  if (!isTauri) {
    // A plain browser holds no host to pull from; open blank.
    const account = useAccountStore.getState().accounts[0];
    if (account !== undefined) {
      useComposerStore
        .getState()
        .newMessage({ id: account.accountId, label: account.displayName }, composition);
    }
    return;
  }
  try {
    const [accountsSnapshot, preferences, hint] = await Promise.all([
      mailboxList(),
      settings(),
      draftStatus(),
    ]);
    useAccountStore.getState().applySnapshot(accountsSnapshot);
    useSettingsStore.getState().applySnapshot(preferences);
    useDraftStore.getState().applySnapshot(hint);
    const request = await takeComposeRequest();
    if (request !== null) {
      useComposeRequestStore.getState().dismiss();
      useComposerStore
        .getState()
        .openRequest(request, fromLabelFor(request.from), composition);
      return;
    }
    const account = useAccountStore.getState().accounts[0];
    if (account !== undefined) {
      useComposerStore
        .getState()
        .newMessage({ id: account.accountId, label: account.displayName }, composition);
    }
  } catch (err) {
    console.error('composer: the window hydration failed', err);
  }
}

interface ComposerWindowProps {
  /** The composition this window holds, from the URL. */
  composition: CompositionId;
}

export function ComposerWindow({ composition }: ComposerWindowProps) {
  const open = useComposerStore((s) => s.open && s.composition !== null);
  const subject = useComposerStore((s) => s.subject);

  useEffect(() => {
    void openComposerSession(composition);
  }, [composition]);

  // The save hint's surface, the one live feed the window needs.
  useEffect(() => {
    if (!isTauri) {
      return;
    }
    const registry = createListenerRegistry();
    void registry.safeListen<{ surface: Surface }>(
      'surface_changed',
      (event) => {
        if (event.payload.surface === 'draft_status') {
          void pullSurface('draft_status');
        }
      },
      'surface_changed',
    );
    return () => registry.disposeAll();
  }, []);

  // The composition leaving the store (a sent, discarded, or closed
  // composition) closes the window it lived in.
  useEffect(
    () =>
      useComposerStore.subscribe((state, previous) => {
        if (previous.open && previous.composition !== null && !state.open && isTauri) {
          void getCurrentWindow()
            .close()
            .catch(() => undefined);
        }
      }),
    [],
  );

  return (
    <div className="flex h-full flex-col bg-background font-ui text-foreground">
      <ComposerTitleBar title={subject === '' ? 'New message' : subject} />
      {open ? (
        <ComposerPane showClose />
      ) : (
        <div
          role="status"
          className="flex flex-1 items-center justify-center text-sm text-muted-foreground"
        >
          Preparing the composer
        </div>
      )}
    </div>
  );
}
