// SPDX-License-Identifier: GPL-3.0-only
// The composer's submit path, split from the store for the line cap: the
// submit intent the composition's origin calls for, the staged bytes the
// standing rows owe the send, and the refusal sentences a submit can
// answer with. Pure helpers; the store holds the state and the gesture.

import type { ComposerBlob, Intent } from '@/services/ipc';
import { intents } from '@/services/ipc';

import { editorToDocument, type EditorDocument } from './documentWire';
import type { CompositionRequest } from './composerOpening';

/** A refused send in words the pane can show, whatever the host handed back. */
export function sendErrorMessage(err: unknown): string {
  if (err instanceof Error) {
    return err.message;
  }
  if (typeof err === 'string') {
    return err;
  }
  return 'The message was not sent';
}

/** A reply whose original arrived with no Message-ID: nothing to thread under. */
export const REPLY_WITHOUT_ORIGIN = 'The original message has no Message-ID to reply under';

/**
 * A send refused while a shared file stands pending: the byte-staging read
 * seam is the row's named remainder, and a message that names an
 * attachment it does not carry would be a lie.
 */
export const SHARES_NOT_ATTACHED =
  'Shared files cannot be attached yet. Remove them to send this message.';

/** The state the submit path reads at click time. */
export interface SubmitState {
  request: CompositionRequest | null;
  from: string;
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  editor: EditorDocument;
  files: Record<string, File>;
}

/**
 * The submit intent the composition's origin calls for. A plain composition
 * sends as a fresh mail; one a request opened sends through its own threaded
 * submit, the reply under its original, the forward answering no one. The
 * threaded wires carry no bcc and the forward's no cc either, so those fields
 * are the plain send's alone. The blobs hold the standing staged rows' bytes,
 * read at click time.
 */
export async function submitIntent(state: SubmitState): Promise<Intent> {
  const document = editorToDocument(state.editor);
  const blobs = await stagedBlobs(state);
  if (state.request === null) {
    return intents.submitRichMail({
      from: state.from,
      to: state.to,
      cc: state.cc,
      bcc: state.bcc,
      subject: state.subject,
      document,
      blobs,
    });
  }
  if (state.request.kind === 'forward') {
    return intents.submitRichForward({
      from: state.from,
      to: state.to,
      subject: state.subject,
      document,
      blobs,
    });
  }
  return intents.submitRichReply({
    from: state.from,
    to: state.to,
    cc: state.cc,
    in_reply_to: state.request.inReplyTo ?? '',
    subject: state.subject,
    document,
    blobs,
  });
}

/** The bytes of every staged file whose row still stands, keyed by the
 *  handle the document names. A removed row drops out here; a forward's
 *  pre-attached originals hold no staged file, so the core resolves them. */
async function stagedBlobs(state: {
  editor: EditorDocument;
  files: Record<string, File>;
}): Promise<ComposerBlob[]> {
  const standing = new Set(
    state.editor.attachments
      .map((row) => row.blob)
      .filter((blob): blob is string => blob !== null),
  );
  const blobs: ComposerBlob[] = [];
  for (const [handle, file] of Object.entries(state.files)) {
    if (standing.has(handle)) {
      blobs.push({ handle, bytes: Array.from(new Uint8Array(await file.arrayBuffer())) });
    }
  }
  return blobs;
}
