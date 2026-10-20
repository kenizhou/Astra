// SPDX-License-Identifier: GPL-3.0-only
// The mailto flow: the reading pane's scheme predicate and raw ask (the
// host owns the parse, so only the link crosses up), the subscription the
// composer opens from (the host's event carries the parsed prefill under
// the default send account it resolved), and the route a landed open
// takes: the New message entry's own, a pop-out window under Tauri with
// the prefill riding the URL, the inline composer outside it.

import { emit, listen } from '@tauri-apps/api/event';

import { isTauri } from '@/events/surfaceEvents';

import { mintCompositionId, useComposerStore, type MailtoOpen } from './composerStore';
import { openComposerWindow } from './composeWindow';
import { fromLabelFor } from './useComposeRequest';

/**
 * Whether an anchor's href is a mailto link, whatever the scheme's case.
 * Routing only: the link itself is never parsed here, the host hands it to
 * the engine's parser.
 */
export function isMailtoHref(href: string): boolean {
  return /^mailto:/i.test(href);
}

/**
 * Hands a clicked mailto link to the host raw: the parse belongs to the
 * engine, so the ask carries the link alone and the answer comes back
 * through {@link onMailtoOpen}.
 */
export function askMailtoOpen(uri: string): Promise<void> {
  return emit('mailto_request', { uri });
}

/**
 * Subscribes to the host's mailto opens and resolves its unlistener; each
 * payload is the parsed prefill under the default send account, ready for
 * {@link openMailtoComposer}.
 */
export function onMailtoOpen(handler: (open: MailtoOpen) => void): Promise<() => void> {
  return listen<MailtoOpen>('mailto_open', (event) => {
    handler(event.payload);
  });
}

/**
 * The mailto route: the New message entry's own behaviour, a pop-out
 * window on a minted composition under Tauri (the prefill riding the URL,
 * titled by its subject) and the inline composer outside it. The prefill
 * seeds the same normalised fields a hand-typed open would hold.
 */
export function openMailtoComposer(prefill: MailtoOpen): Promise<void> {
  if (!isTauri) {
    useComposerStore
      .getState()
      .newMessage({ id: prefill.from, label: fromLabelFor(prefill.from) }, undefined, prefill);
    return Promise.resolve();
  }
  const composition = mintCompositionId();
  const title = prefill.subject === '' ? 'New message' : prefill.subject;
  return openComposerWindow(composition, title, undefined, prefill);
}
