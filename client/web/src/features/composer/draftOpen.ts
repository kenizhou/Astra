// SPDX-License-Identifier: GPL-3.0-only
// The draft row's opening path: the by-key resume read the Drafts folder's
// Open verbs drive, beside the routes that land it (the inline composer's
// pane, or its own window under Tauri). A standing inline composition hands
// its whole current draft over as a final save first, the same discipline a
// compose request's landing runs, so the row's open takes the pane with the
// old words kept in Drafts rather than dropped.

import { invoke } from '@tauri-apps/api/core';

import { dispatch, intents, type MessageRef, type ResumeView } from '@/services/ipc';
import { isTauri } from '@/events/surfaceEvents';

import { fromLabelFor } from './useComposeRequest';
import { openComposerWindow } from './composeWindow';
import { editorToDocument } from './documentWire';
import { mintCompositionId, useComposerStore } from './composerStore';

/**
 * Opens the Drafts row's named copy into the composition the caller minted:
 * the by-key read, answering the record to hydrate from or null when the key
 * names nothing the account holds, a message that is not a draft, or a body
 * that will not read.
 */
export function draftResumeByKey(
  message: MessageRef,
  composition: string,
): Promise<ResumeView | null> {
  return invoke('draft_resume_by_key', { message, composition });
}

/** Hands a standing inline composition over as a final save and closes the
 *  pane, the row's open's own clearing of it: the words stay in Drafts
 *  whatever the resume below answers. */
function handOverStanding(): void {
  const standing = useComposerStore.getState();
  if (standing.open && standing.composition !== null) {
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
  useComposerStore.getState().closePane();
}

/** Opens the Drafts row's named copy into the inline composer's pane. */
export async function openDraftRow(message: MessageRef): Promise<void> {
  handOverStanding();
  const composition = mintCompositionId();
  const view = await draftResumeByKey(message, composition);
  if (view === null) {
    return;
  }
  useComposerStore
    .getState()
    .hydrate(view, { id: view.account, label: fromLabelFor(view.account) }, composition);
}

/** Opens the Drafts row's named copy into a composer window of its own, the
 *  reading row's own-window verb's shape; outside Tauri the pane serves it. */
export async function openDraftRowWindow(message: MessageRef, subject: string): Promise<void> {
  if (!isTauri) {
    await openDraftRow(message);
    return;
  }
  const composition = mintCompositionId();
  await openComposerWindow(
    composition,
    subject === '' ? 'New message' : subject,
    message.account,
    undefined,
    undefined,
    message.key,
  );
}
