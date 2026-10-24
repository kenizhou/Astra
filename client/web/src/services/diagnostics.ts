// SPDX-License-Identifier: GPL-3.0-only
// The diagnostics half of the IPC wire: the log tail the Diagnostics
// viewer renders and the log export over the host's save seam. The tail
// answers the current file's last lines (oldest of the kept first, so
// the newest lands last); the export answers the shared snake_case
// outcome family the attachment and eml saves answer. Split into this
// owning service, never ipc.ts, which sits at the line cap.

import { invoke } from '@tauri-apps/api/core';

import type { SaveAttachmentOutcome } from './attachments';

/** The line count the Diagnostics viewer asks for: a screenful. */
export const TAIL_LINES = 200;

/**
 * Reads the current log file's last `n` lines, oldest of the kept first,
 * so the newest lands last. No path crosses beyond the lines themselves.
 */
export function logTail(n: number): Promise<string[]> {
  return invoke('log_tail', { n });
}

/**
 * Exports the current log file: the host asks where to put it with the
 * file's own name, writes its whole bytes, and answers the typed outcome
 * the surface shows.
 */
export function logExport(): Promise<SaveAttachmentOutcome> {
  return invoke('log_export');
}
