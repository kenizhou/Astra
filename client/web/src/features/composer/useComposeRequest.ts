// SPDX-License-Identifier: GPL-3.0-only
// The compose request flow: the surface pull lands a published request in
// the compose request store, and the hook here opens the composer on it,
// pane shut or open. A request replaces whatever composition is standing
// open through the Close discipline (a final save that forgets the old
// composition), so its words sit stored and nothing is discarded silently;
// the request's own composition is minted fresh, its prefill shown as
// fields, its quote held as retained content, and its threading header kept
// until the send spends it.

import { useEffect } from 'react';

import { dispatch, intents, type ComposeRequestSnapshot } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';

import { useComposerStore } from './composerStore';
import { editorToDocument } from './documentWire';

/** The label the composer's From shows for the request's own account. */
function fromLabelFor(account: string): string {
  const known = useAccountStore
    .getState()
    .accounts.find((row) => row.accountId === account);
  return known?.displayName ?? account;
}

/** Opens the composer on one published compose request. */
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

/** Opens the composer whenever a compose request lands, pane shut or open. */
export function useComposeRequestHydration(): void {
  useEffect(
    () =>
      useComposeRequestStore.subscribe((state, previous) => {
        const { snapshot } = state;
        if (snapshot !== null && snapshot !== previous.snapshot) {
          openComposeRequest(snapshot);
        }
      }),
    [],
  );
}
