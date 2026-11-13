// SPDX-License-Identifier: GPL-3.0-only
// The reminder half of the IPC wire: the command that asks to be notified
// about a message at a wake instant. Split from ipc.ts to keep that file
// under the line cap (the wire types ride here, never there), like the
// snooze half before it. A reminder is a host command, not an intent: the
// core records the provider key, subject, and sender; the message never
// moves and is never flagged, and the wake timer notifies at the instant.
// The picker's wake choices reuse the snooze preset math (snoozeChoices).

import { invoke } from '@tauri-apps/api/core';

import type { MessageRef } from './ipc';

/** Reminds one message at the wake instant (unix seconds): the core records
 *  the provider key, subject, and sender; the message stays put. */
export function remindMessage(message: MessageRef, wakeAt: number): Promise<void> {
  return invoke('remind_message', {
    account: message.account,
    key: message.key,
    wakeAt,
  });
}
