// SPDX-License-Identifier: GPL-3.0-only
// The pop-out composer window's root: main.tsx mounts it in place of the app
// shell when the URL carries a composition id. It hydrates its own session
// from the host: the accounts and settings pulls (the settings apply the
// theme class), the draft-status pull for the save hint, the transfer's
// resumed draft, the mailto prefill straight off the URL, and the consuming
// take of the pending compose request the reply and forward flows published
// for exactly this window; no pending request means a plain new message on
// the first account. The draft-status surface keeps the status bar's save
// hint live, and the composition leaving the store (a send, a discard, a
// close) closes the OS window.

import { useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { createListenerRegistry, isTauri, pullSurface, type Surface } from '@/events/surfaceEvents';
import {
  draftResume,
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
import { defaultSendAccount } from './composeWindow';
import { useComposerStore, type MailtoOpen } from './composerStore';
import type { SharedFile } from './shareOpen';
import { fromLabelFor } from './useComposeRequest';

// One hydration per window, StrictMode safe: the double-mounted effect's
// second run finds the session already taken and returns.
let hydratedFor: CompositionId | null = null;

/** Opens the window's composition: the transfer's resumed draft, the
 *  URL's mailto prefill, the pending request's, the URL share set's, or a
 *  blank one. */
async function openComposerSession(
  composition: CompositionId,
  resumeAccount: string | null,
  prefill: MailtoOpen | null,
  shares: SharedFile[] | null,
): Promise<void> {
  if (hydratedFor === composition) {
    return;
  }
  hydratedFor = composition;
  if (!isTauri) {
    // A plain browser holds no host to pull from; open blank from the
    // default send account the store already holds.
    const account = defaultSendAccount();
    if (account !== undefined) {
      useComposerStore.getState().newMessage(account, composition);
      if (shares !== null) {
        useComposerStore.getState().seedShares(shares);
      }
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
    if (resumeAccount !== null) {
      // The dock-to-window transfer: the dock saved its draft and closed
      // behind it, so the resume reads exactly those words back. The wire
      // carries a plain-text body (ResumeView.body_text), so rich editor
      // formatting flattens to plain paragraphs on the transfer. A null
      // answer (the draft is already gone) falls through to the blank
      // tail rather than stranding the window on its placeholder.
      const view = await draftResume(resumeAccount, composition);
      if (view !== null) {
        useComposerStore
          .getState()
          .hydrate(
            view,
            { id: view.account, label: fromLabelFor(view.account) },
            composition,
          );
        return;
      }
    }
    if (prefill !== null) {
      // A mailto window: the host already parsed the link and resolved the
      // default send account, so the prefill seeds the composition here and
      // no request is asked for or spent.
      useComposerStore
        .getState()
        .newMessage({ id: prefill.from, label: fromLabelFor(prefill.from) }, composition, prefill);
      return;
    }
    if (shares !== null) {
      // A share window: a blank composition carrying the hand-off's files
      // as the pending set, no request asked for or spent. The names seed
      // the rows; the paths stay in the pending state alone.
      const account = defaultSendAccount();
      if (account !== undefined) {
        useComposerStore.getState().newMessage(account, composition);
        useComposerStore.getState().seedShares(shares);
      }
      return;
    }
    const request = await takeComposeRequest();
    if (request !== null) {
      useComposeRequestStore.getState().dismiss();
      useComposerStore
        .getState()
        .openRequest(request, fromLabelFor(request.from), composition);
      return;
    }
    const account = defaultSendAccount();
    if (account !== undefined) {
      useComposerStore.getState().newMessage(account, composition);
    }
  } catch (err) {
    console.error('composer: the window hydration failed', err);
  }
}

interface ComposerWindowProps {
  /** The composition this window holds, from the URL. */
  composition: CompositionId;
  /** The account a transfer resumes the latest draft of, from the URL. */
  resumeAccount?: string | null;
  /** The mailto prefill this window opens from, from the URL. */
  prefill?: MailtoOpen | null;
  /** The pending share set this window opens with, from the URL. */
  shares?: SharedFile[] | null;
}

export function ComposerWindow({
  composition,
  resumeAccount = null,
  prefill = null,
  shares = null,
}: ComposerWindowProps) {
  const open = useComposerStore((s) => s.open && s.composition !== null);
  const subject = useComposerStore((s) => s.subject);

  useEffect(() => {
    void openComposerSession(composition, resumeAccount, prefill, shares);
  }, [composition, resumeAccount, prefill, shares]);

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
