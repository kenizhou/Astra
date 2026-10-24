// SPDX-License-Identifier: GPL-3.0-only
// The share flow: the host's share channel (the mailto sibling) hands the
// filtered shared files over, and this route opens the compose prefill
// with the file's NAME. The path rides the composition's pending share
// set and never renders and never logs: the byte-staging read seam is the
// row's named remainder, so the files stand pending, their names shown,
// their bytes unstaged, and the send refuses honestly while any stands.

import { listen } from '@tauri-apps/api/event';

import { isTauri } from '@/events/surfaceEvents';

import { mintCompositionId, useComposerStore } from './composerStore';
import { defaultSendAccount, openComposerWindow } from './composeWindow';

/** One shared file as the host's channel carries it. */
export interface SharedFile {
  /** The file's own name, as the pending row shows it. */
  name: string;
  /** The file's path, as the OS handed it over; held, never shown. */
  path: string;
}

/**
 * Subscribes to the host's share hand-offs and resolves its unlistener;
 * each payload is the filtered shared file set, ready for
 * {@link openShareComposer}.
 */
export function onShareOpen(
  handler: (files: SharedFile[]) => void,
): Promise<() => void> {
  return listen<SharedFile[]>('share_open', (event) => {
    handler(event.payload);
  });
}

/**
 * The share route: the New message entry's own behaviour over the shared
 * files' names, a pop-out window on a minted composition under Tauri (the
 * pending set riding the URL, titled by the first file) and the inline
 * composer outside it. Nothing opens without an account to send from or a
 * file to carry.
 */
export function openShareComposer(files: SharedFile[]): Promise<void> {
  const account = defaultSendAccount();
  if (account === undefined || files.length === 0) {
    return Promise.resolve();
  }
  if (!isTauri) {
    useComposerStore.getState().newMessage(account);
    useComposerStore.getState().seedShares(files);
    return Promise.resolve();
  }
  const composition = mintCompositionId();
  const title = files.length === 1 ? files[0]!.name : 'New message';
  return openComposerWindow(composition, title, undefined, undefined, files);
}
