// SPDX-License-Identifier: GPL-3.0-only
// The pop-out composer window's mechanics: the URL that carries the
// composition id alone, the window label the capability wildcard matches,
// and the runtime WebviewWindow the main window opens per composition,
// kylins' composeWindow.ts idiom narrowed to the one thing the URL hands
// over (the prefill itself stays engine-side; the window pulls it through
// the consuming take). The New message route lives here too: a window under
// Tauri, the inline composer outside it, the same fallback kylins takes.

import { isTauri } from '@/events/surfaceEvents';
import type { CompositionId } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';

import { mintCompositionId, useComposerStore } from './composerStore';

/** The query parameter a composer window's URL carries its composition id in. */
const COMPOSER_PARAM = 'composer';

/** The composition id a composer window's URL names; null for the main window. */
export function readComposerParam(search: string): CompositionId | null {
  const id = new URLSearchParams(search).get(COMPOSER_PARAM);
  return id === null || id === '' ? null : id;
}

/** The URL a composer window loads: the bundle with the composition id alone. */
export function composerWindowUrl(composition: CompositionId): string {
  return `index.html?${COMPOSER_PARAM}=${encodeURIComponent(composition)}`;
}

/** The window label a composition's pop-out takes, under the composer-* capability. */
export function composerWindowLabel(composition: CompositionId): string {
  return `composer-${composition}`;
}

/**
 * Opens the pop-out composer window for one minted composition: frameless
 * (the bundle draws its own title bar), centred, resizable down to the
 * composer's minimum. The window hydrates itself from the host; nothing but
 * the id crosses the URL.
 */
export async function openComposerWindow(
  composition: CompositionId,
  title: string,
): Promise<void> {
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const webview = new WebviewWindow(composerWindowLabel(composition), {
    url: composerWindowUrl(composition),
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
