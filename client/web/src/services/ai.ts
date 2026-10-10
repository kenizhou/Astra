// SPDX-License-Identifier: GPL-3.0-only
// The AI replies IPC layer: the command wrappers the reading pane's chips,
// summary, and style surfaces call, the accept that opens the composer on a
// drafted reply, and the draft-pool event subscription that tells the pane a
// message's pool changed. The wire shapes mirror the desktop host's serde
// exactly: a chip crosses as its reply beside its preview, and the event
// payload names the landed message by its account and key pair.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { ChipView, DraftPoolLanded, MessageRef } from './ipc';

// The wire types this service carries, re-exported so this module stays the
// one import point of the AI replies surface.
export type { ChipView, DraftPoolLanded };

/** Pulls the message's smart-reply chips from the draft pool through the gate. */
export function chipsFor(message: MessageRef): Promise<ChipView[]> {
  return invoke('draft_chips', { account: message.account, key: message.key });
}

/** Summarises the message, cached: a hit answers without a model call. */
export function summariseMessage(message: MessageRef): Promise<string> {
  return invoke('summarise', { account: message.account, key: message.key });
}

/** The account's writing-style note, or null when none was built. */
export function accountStyleProfile(account: string): Promise<string | null> {
  return invoke('style_profile', { account });
}

/** Records the reader's dismissal of the message's chips. */
export function dismissChips(message: MessageRef): Promise<void> {
  return invoke('chips_dismiss', { account: message.account, key: message.key });
}

/**
 * Opens the reply composer on one accepted chip: the host builds the reply's
 * own prefill (recipients, subject, threading header, quoted original) and
 * publishes it with the drafted reply already the document's opening
 * paragraphs, so the composer hydrates from the request exactly as it does
 * from a reply's own dispatch.
 */
export function composeFromDraft(message: MessageRef, replyText: string): Promise<void> {
  return invoke('compose_from_draft', {
    account: message.account,
    key: message.key,
    replyText,
  });
}

/**
 * Subscribes to the draft-pool event: the host emits one per landed pool row
 * (drafted or skipped alike), and the handler receives the message it landed
 * for, so the reading pane can pull chips again when the row is its own.
 * Resolves the unlistener.
 */
export function onDraftPoolLanded(handler: (message: MessageRef) => void): Promise<() => void> {
  return listen<DraftPoolLanded>('draft_pool', (event) => {
    handler({ account: event.payload.account, key: event.payload.key });
  });
}
