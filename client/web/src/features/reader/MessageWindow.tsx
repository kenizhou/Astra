// SPDX-License-Identifier: GPL-3.0-only
// The pop-out reading window's root: main.tsx mounts it in place of the app
// shell when the URL carries a message identity. It opens its message
// through the same open_message intent the pane's row click sends, so the
// mark-read rides the host's one guarded open path and lands once whichever
// reader opened it, then hydrates its own session (the reading snapshot,
// the settings, the account tree) and renders the whole reading view over
// the shared component. The open reading moving off its message, or
// closing, closes the OS window.

import { useEffect, useRef } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { createListenerRegistry, isTauri, pullSurface, type Surface } from '@/events/surfaceEvents';
import { ComposerTitleBar } from '@/features/composer/ComposerTitleBar';
import { dispatch, intents, mailboxList, reading, settings, type MessageRef } from '@/services/ipc';
import { ReadingView } from '@/components/reading-pane/ReadingView';
import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useReadingStore } from '@/stores/readingStore';
import { useSettingsStore } from '@/features/settings/settingsStore';

/** Whether the reading snapshot holds this window's message. */
function holdsMessage(
  snapshot: ReturnType<typeof useReadingStore.getState>['snapshot'],
  message: MessageRef,
): boolean {
  return (
    snapshot !== null &&
    snapshot.message.account === message.account &&
    snapshot.message.key === message.key
  );
}

interface MessageWindowProps {
  /** The message this window holds, from the URL. */
  message: MessageRef;
  /** The folder key the list sat in when the window opened, from the URL. */
  folderKey: string | null;
}

export function MessageWindow({ message, folderKey }: MessageWindowProps) {
  const snapshot = useReadingStore((s) => s.snapshot);
  const subject = useReadingStore((s) => s.snapshot?.subject ?? '');
  const hydrated = useRef(false);

  // One hydration per window, StrictMode safe: the double-mounted effect's
  // second run finds the guard set and returns. The open lands first, so
  // the reading pull that follows reads the message this window is for.
  useEffect(() => {
    if (hydrated.current) {
      return;
    }
    hydrated.current = true;
    if (!isTauri) {
      // A plain browser holds no host to pull from; the window stays on its
      // placeholder, and the pane fallback is what opened there anyway.
      return;
    }
    void (async () => {
      try {
        await dispatch(intents.openMessage(message));
        const [accountsSnapshot, preferences, openReading] = await Promise.all([
          mailboxList(),
          settings(),
          reading(),
        ]);
        useAccountStore.getState().applySnapshot(accountsSnapshot);
        useFolderStore.getState().applySnapshot(accountsSnapshot);
        useSettingsStore.getState().applySnapshot(preferences);
        useReadingStore.getState().applySnapshot(openReading);
      } catch (err) {
        console.error('message-window: the window hydration failed', err);
      }
    })();
  }, [message]);

  // The surfaces this window keeps live: the reading snapshot it renders,
  // the settings its header formats by, and the account tree its junk
  // judgement reads.
  useEffect(() => {
    if (!isTauri) {
      return;
    }
    const registry = createListenerRegistry();
    void registry.safeListen<{ surface: Surface }>(
      'surface_changed',
      (event) => {
        const { surface } = event.payload;
        if (
          surface === 'reading' ||
          surface === 'settings' ||
          surface === 'mailbox_list'
        ) {
          void pullSurface(surface);
        }
      },
      'surface_changed',
    );
    return () => registry.disposeAll();
  }, []);

  // The window closes when its message closes: the reading state no longer
  // holding the window's message is the closing, whichever way it happened
  // (a close intent, a refresh that dropped the message, another reader
  // opening something else).
  useEffect(
    () =>
      useReadingStore.subscribe((state) => {
        if (!holdsMessage(state.snapshot, message) && isTauri) {
          void getCurrentWindow()
            .close()
            .catch(() => undefined);
        }
      }),
    [message],
  );

  const open = holdsMessage(snapshot, message);

  return (
    <div className="flex h-full flex-col bg-background font-ui text-foreground">
      <ComposerTitleBar title={subject === '' ? 'Message' : subject} />
      {open ? (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-[var(--paper)]">
          <ReadingView
            openedFromFolder={
              folderKey !== null ? { account: message.account, key: folderKey } : null
            }
          />
        </div>
      ) : (
        <div
          role="status"
          className="flex flex-1 items-center justify-center text-sm text-muted-foreground"
        >
          Opening the message
        </div>
      )}
    </div>
  );
}
