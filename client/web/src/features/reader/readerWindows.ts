// SPDX-License-Identifier: GPL-3.0-only
// The reading windows' mechanics: the URL params that carry a message
// identity (the account and the key as their own params, because both are
// opaque strings no separator can be guaranteed free of), the labels the
// message-* capability wildcard matches, and the runtime windows the main
// window opens: the own window over one message, and the print surface over
// the same identity. Outside Tauri the own window falls back to opening the
// message in the pane, the inline reader a plain browser has.

import { isTauri } from '@/events/surfaceEvents';
import { dispatch, intents, type MessageRef } from '@/services/ipc';

/** The query parameter a message window's URL carries the account in. */
const ACCOUNT_PARAM = 'message-account';

/** The query parameter a message window's URL carries the provider key in. */
const KEY_PARAM = 'message-key';

/** The query parameter carrying the folder key the list sat in at open. */
const FOLDER_PARAM = 'folder';

/** The query parameter marking the print surface. */
const PRINT_PARAM = 'print';

/** What one message window's URL carries. */
export interface MessageWindowParams {
  message: MessageRef;
  folderKey: string | null;
  print: boolean;
}

/** The message identity a message window's URL names; null for the main window. */
export function readMessageWindowParams(search: string): MessageWindowParams | null {
  const params = new URLSearchParams(search);
  const account = params.get(ACCOUNT_PARAM);
  const key = params.get(KEY_PARAM);
  if (account === null || account === '' || key === null || key === '') {
    return null;
  }
  const folder = params.get(FOLDER_PARAM);
  return {
    message: { account, key },
    folderKey: folder === null || folder === '' ? null : folder,
    print: params.get(PRINT_PARAM) === '1',
  };
}

/** The URL a message window loads: the identity alone, plus the folder key
 *  when one crossed. */
export function messageWindowUrl(message: MessageRef, folderKey?: string | null): string {
  const base = `index.html?${ACCOUNT_PARAM}=${encodeURIComponent(message.account)}&${KEY_PARAM}=${encodeURIComponent(message.key)}`;
  return folderKey === undefined || folderKey === null
    ? base
    : `${base}&${FOLDER_PARAM}=${encodeURIComponent(folderKey)}`;
}

/** The URL the print window loads over the same identity. */
export function printWindowUrl(message: MessageRef): string {
  return `${messageWindowUrl(message)}&${PRINT_PARAM}=1`;
}

/** The window label an own-window takes, under the message-* wildcard. */
export function messageWindowLabel(id: string): string {
  return `message-${id}`;
}

/** The window label the print surface takes, under the same wildcard. */
export function printWindowLabel(id: string): string {
  return `message-print-${id}`;
}

/** Mints the window id an own-window or print window opens under. */
function mintWindowId(): string {
  return crypto.randomUUID();
}

/**
 * Opens the pop-out window over one message: frameless, centred, resizable
 * down to the reading view's minimum, titled by the subject. The folder the
 * list sat in crosses the URL so the window can judge the junk-folder
 * context its Not-spam verb needs. Outside Tauri a window cannot open, so
 * the message opens in the pane instead, the inline reader that is always
 * there.
 */
export async function openMessageWindow(
  message: MessageRef,
  title: string,
  folderKey: string | null,
): Promise<void> {
  if (!isTauri) {
    await dispatch(intents.openMessage(message));
    return;
  }
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const webview = new WebviewWindow(messageWindowLabel(mintWindowId()), {
    url: messageWindowUrl(message, folderKey),
    title,
    width: 760,
    height: 640,
    minWidth: 480,
    minHeight: 420,
    center: true,
    decorations: false,
    resizable: true,
  });
  await webview.once('tauri://error', (event) => {
    console.error('message-window: the pop-out window failed to open', event);
  });
}

/**
 * Opens the print surface over one message: the print-optimised window that
 * renders the document and prints itself, the platform dialog being the only
 * printer the webview offers.
 */
export async function openPrintWindow(message: MessageRef): Promise<void> {
  const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const webview = new WebviewWindow(printWindowLabel(mintWindowId()), {
    url: printWindowUrl(message),
    title: 'Print',
    width: 720,
    height: 900,
    center: true,
    resizable: true,
  });
  await webview.once('tauri://error', (event) => {
    console.error('message-window: the print window failed to open', event);
  });
}
