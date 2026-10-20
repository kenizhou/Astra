// SPDX-License-Identifier: GPL-3.0-only
// The attachments IPC layer: the reading pane's Save As. The wire shapes
// mirror the desktop host's serde exactly: the outcome crosses as its
// snake_case tag with the saved path or the refusal's reason, and the
// reader's cancel as the bare word. The composer's picker is no wire at
// all: the webview's own multi-select file input hands the composer File
// objects, so the bytes never round-trip through the host.

import { invoke } from '@tauri-apps/api/core';

import type { MessageRef } from './ipc';

/** The typed answer to one Save As, as the reading pane shows it. */
export type SaveAttachmentOutcome =
  /** The bytes landed at the path the reader chose; the path to show. */
  | { saved: { path: string } }
  /** The reader closed the dialog without choosing; nothing was written. */
  | 'cancelled'
  /** The save could not happen; the sentence names why, ready to show. */
  | { failed: { reason: string } };

/**
 * Saves one attachment of the open message: the host reads the decoded
 * bytes, asks where to put them with the row's file name as the
 * suggestion, writes them, and answers the typed outcome.
 */
export function saveAttachment(
  message: MessageRef,
  attachment: number,
): Promise<SaveAttachmentOutcome> {
  return invoke('save_attachment', {
    account: message.account,
    key: message.key,
    attachment,
  });
}
