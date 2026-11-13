// SPDX-License-Identifier: GPL-3.0-only
// The crypto receive IPC layer (SP4c-3c): the reading pane's password
// re-entry wire. The prompt affordance the reading snapshot carries and the
// passphrase submission both mirror the desktop host's serde exactly: the
// affordance crosses as the shared snake_case word, and the submission's
// answer is the affordance the refreshed snapshot now carries (null once the
// passphrase decrypts). Split from ipc.ts to keep that file under the line
// cap, and re-exported whole through it, so ipc stays the one import point of
// the wire. The passphrase crosses into host memory only: it is never logged
// or persisted, and a cancel never calls the submission at all.

import { invoke } from '@tauri-apps/api/core';

import type { MessageRef } from './ipc';

/**
 * Where the passphrase prompt stands for a passphrase-protected message: the
 * affordance the reading pane's password dialog reads. A transient interaction,
 * never a persisted state.
 */
export type PasswordPromptState =
  /** The message needs a passphrase to open: the dialog collects one. */
  | 'needs_password'
  /** The supplied passphrase fit no slot: the dialog re-prompts. */
  | 'wrong_password';

/**
 * Submits the reader's passphrase for the open passphrase-protected message:
 * the host re-runs the password orchestrator over a fresh source and refreshes
 * the reading snapshot. The answer is the prompt affordance the snapshot now
 * carries: null once the passphrase decrypts (the words render, the dialog
 * closes) or on a structural failure, and 'wrong_password' to re-prompt. The
 * passphrase stays in-process: it is never logged or persisted.
 */
export function submitReadingPassword(
  message: MessageRef,
  passphrase: string,
): Promise<PasswordPromptState | null> {
  return invoke('submit_reading_password', {
    account: message.account,
    key: message.key,
    passphrase,
  });
}