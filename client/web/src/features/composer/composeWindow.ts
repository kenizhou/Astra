// SPDX-License-Identifier: GPL-3.0-only
// The pop-out composer window's mechanics: the URL that carries the
// composition id alone, the window label the capability wildcard matches,
// and the runtime WebviewWindow the main window opens per composition,
// kylins' composeWindow.ts idiom narrowed to the one thing the URL hands
// over (the prefill itself stays engine-side; the window pulls it through
// the consuming take). The New message route lives here too: a window under
// Tauri, the inline composer outside it, the same fallback kylins takes.

import { isTauri } from '@/events/surfaceEvents';
import { dispatch, intents, type CompositionId } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';

import { mintCompositionId, useComposerStore } from './composerStore';
import { editorToDocument } from './documentWire';

/** The query parameter a composer window's URL carries its composition id in. */
const COMPOSER_PARAM = 'composer';

/** The query parameter a transfer marks the resume account in. */
const RESUME_PARAM = 'resume';

/** The composition id a composer window's URL names; null for the main window. */
export function readComposerParam(search: string): CompositionId | null {
  const id = new URLSearchParams(search).get(COMPOSER_PARAM);
  return id === null || id === '' ? null : id;
}

/** The account a transfer window resumes the latest draft of; null for a
 *  plain pop-out. */
export function readResumeParam(search: string): string | null {
  const account = new URLSearchParams(search).get(RESUME_PARAM);
  return account === null || account === '' ? null : account;
}

/** The URL a composer window loads: the bundle with the composition id, and
 *  the resume marker when the dock's draft transfers. */
export function composerWindowUrl(composition: CompositionId, resumeAccount?: string): string {
  const base = `index.html?${COMPOSER_PARAM}=${encodeURIComponent(composition)}`;
  return resumeAccount === undefined
    ? base
    : `${base}&${RESUME_PARAM}=${encodeURIComponent(resumeAccount)}`;
}

/** The window label a composition's pop-out takes, under the composer-* capability. */
export function composerWindowLabel(composition: CompositionId): string {
  return `composer-${composition}`;
}

/**
 * Opens the pop-out composer window for one minted composition: frameless
 * (the bundle draws its own title bar), centred, resizable down to the
 * composer's minimum. The window hydrates itself from the host; nothing but
 * the id crosses the URL. A dock-to-window transfer marks the resume
 * account on the URL, so the window resumes the draft the dock just saved
 * rather than opening blank.
 */
export async function openComposerWindow(
  composition: CompositionId,
  title: string,
  resumeAccount?: string,
): Promise<void> {
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const webview = new WebviewWindow(composerWindowLabel(composition), {
    url: composerWindowUrl(composition, resumeAccount),
    title,
    width: 720,
    height: 640,
    minWidth: 600,
    minHeight: 480,
    center: true,
    decorations: false,
    resizable: true,
  });
  await webview.once('tauri://error', (event) => {
    console.error('composer: the pop-out window failed to open', event);
  });
}

/**
 * The New message route: a pop-out window under Tauri, the inline composer
 * outside it (a plain browser holds no windows). The window hydrates blank;
 * only the minted id crosses.
 */
export function openNewMessageComposer(): Promise<void> {
  const account = useAccountStore.getState().accounts[0];
  if (account === undefined) {
    return Promise.resolve();
  }
  if (!isTauri) {
    useComposerStore.getState().newMessage({ id: account.accountId, label: account.displayName });
    return Promise.resolve();
  }
  const composition = mintCompositionId();
  return openComposerWindow(composition, 'New message');
}

/**
 * The dock-to-window transfer: the draft saves (and closes host-side), the
 * dock closes locally, and the composition's own window opens with the
 * account marked for its resume. The save settles before the window opens,
 * so the window's resume never races the words it reads back.
 */
export async function popOutComposer(): Promise<void> {
  const state = useComposerStore.getState();
  if (state.composition === null) {
    return;
  }
  await dispatch(
    intents.saveDraft({
      composition: state.composition,
      from: state.from,
      to: state.to,
      cc: state.cc,
      bcc: state.bcc,
      subject: state.subject,
      document: editorToDocument(state.editor),
      then_close: true,
    }),
  );
  const composition = mintCompositionId();
  const title = state.subject === '' ? 'New message' : state.subject;
  useComposerStore.getState().closePane();
  await openComposerWindow(composition, title, state.from);
}
