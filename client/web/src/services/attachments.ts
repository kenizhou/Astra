// SPDX-License-Identifier: GPL-3.0-only
// The reading saves' IPC layer: the reading pane's two Save As paths, the
// attachment save and the whole-message .eml save. Both wires mirror the
// desktop host's serde exactly: the outcomes cross as the shared
// snake_case family with the saved path or the refusal's reason, and the
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

/**
 * Saves one message as an `.eml`: the host reads the engine's source bytes
 * (never a re-serialization), asks where to put them with the subject's
 * sanitised name as the suggestion, writes them, and answers the same typed
 * outcome family the attachment save answers.
 */
export function saveEml(message: MessageRef): Promise<SaveAttachmentOutcome> {
  return invoke('eml_save', {
    account: message.account,
    key: message.key,
  });
}
