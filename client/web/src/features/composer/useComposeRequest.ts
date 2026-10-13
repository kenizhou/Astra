// SPDX-License-Identifier: GPL-3.0-only
// The compose request flow: the surface pull lands a published request in
// the compose request store, and the hook here routes it. The quick-reply
// strip's expectation opens the request in the docked inline composer, the
// strip's own gesture; every other landed request (the ribbon's reply and
// forward verbs, an accepted smart-reply chip, an agent's draft) opens a
// pop-out composer window on a freshly minted composition id, and the window
// hydrates itself through the consuming take. Outside Tauri there are no
// windows, so the route falls back to the inline composer. A request opened
// inline empties the engine's slot behind it, so no later pop-out re-reads
// the spent request.

import { useEffect } from 'react';

import {
  dispatch,
  intents,
  takeComposeRequest,
  type ComposeRequestSnapshot,
} from '@/services/ipc';
import { isTauri } from '@/events/surfaceEvents';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';

import { mintCompositionId, useComposerStore } from './composerStore';
import { openComposerWindow } from './composeWindow';
import { editorToDocument } from './documentWire';

/** The label the composer's From shows for the request's own account. */
export function fromLabelFor(account: string): string {
  const known = useAccountStore
    .getState()
    .accounts.find((row) => row.accountId === account);
  return known?.displayName ?? account;
}

/**
 * Opens the inline composer on one published compose request. A standing
 * inline composition hands its whole current draft over as a final save
 * first, so the request takes the strip with the old words kept in Drafts
 * rather than dropped.
 */
export function openComposeRequest(request: ComposeRequestSnapshot): void {
  const standing = useComposerStore.getState();
  if (standing.open && standing.composition !== null) {
    // The Close discipline: the standing composition hands its whole current
    // draft over as a final save, so the request takes the pane with the
    // old words kept in Drafts rather than dropped.
    void dispatch(
      intents.saveDraft({
        composition: standing.composition,
        from: standing.from,
        to: standing.to,
        cc: standing.cc,
        bcc: standing.bcc,
        subject: standing.subject,
        document: editorToDocument(standing.editor),
        then_close: true,
      }),
    );
  }
  useComposerStore.getState().openRequest(request, fromLabelFor(request.from));
}

/** Routes one published compose request to the surface that asked for it. */
function routeComposeRequest(request: ComposeRequestSnapshot): void {
  if (useComposerStore.getState().inlineExpected) {
    openComposeRequest(request);
    // The slot empties behind the inline opening, so a later pop-out window
    // never re-reads this spent request.
    void takeComposeRequest().catch((err: unknown) => {
      console.error('compose: the request slot could not be consumed', err);
    });
    return;
  }
  if (!isTauri) {
    openComposeRequest(request);
    return;
  }
  const composition = mintCompositionId();
  void openComposerWindow(
    composition,
    request.subject === '' ? 'New message' : request.subject,
  );
  // The request is the pop-out's now: the main window drops its own copy,
  // and the window's take empties the slot engine-side.
  useComposeRequestStore.getState().dismiss();
}

/** Routes every landed compose request, pop-out by default. */
export function useComposeRequestHydration(): void {
  useEffect(
    () =>
      useComposeRequestStore.subscribe((state, previous) => {
        const { snapshot } = state;
        if (snapshot !== null && snapshot !== previous.snapshot) {
          routeComposeRequest(snapshot);
        }
      }),
    [],
  );
}
