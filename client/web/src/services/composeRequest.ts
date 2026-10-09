// SPDX-License-Identifier: GPL-3.0-only
// The compose half of the IPC wire: the compose family, its two threaded
// submits, and the request surface's snapshot types, with their constructors.
// Split from ipc.ts to keep that file under the line cap, like the sending
// half before it, and re-exported whole through it, so ipc stays the one
// import point of the wire. The literals are pinned in composeRequest.test.ts
// against the JSON astra-app's protocol_tests.rs and protocol_submit_tests.rs
// and astra-viewmodel's compose_request_tests.rs assert.

import type { ComposerDocument } from './composerDocument';
import type { MessageRef } from './ipc';
import type { ComposerBlob } from './sending';

/** Which composition a compose request opens; crosses as a plain snake_case word. */
export type ComposeKind = 'reply' | 'reply_all' | 'forward';

/**
 * What the reading pane asks the runtime to compose from one message, nested
 * under the `compose` tag of the intent that carries it. Every verb names the
 * original by reference, the account and provider key bound together.
 */
export type ComposeIntent =
  | { reply: { message: MessageRef } }
  | { reply_all: { message: MessageRef } }
  | { forward: { message: MessageRef } };

/** The fields a reply submit carries, beside the composer document itself. */
export interface ReplySubmission {
  /** The account to send from: the composer's From dropdown. */
  from: string;
  /** The `To` recipients, comma-separated. */
  to: string;
  /** The `Cc` recipients, comma-separated; may be empty. */
  cc: string;
  /**
   * The original's own `Message-ID`, the header the reply answers: the
   * compose request's `in_reply_to` handed back as a bare string.
   */
  in_reply_to: string;
  /** The subject line; may be empty. */
  subject: string;
  /** The shared composer document. */
  document: ComposerDocument;
  /** Host-resolved bytes for every attachment handle; empty with no attachments. */
  blobs: ComposerBlob[];
}

/** The fields a forward submit carries, beside the composer document itself. */
export interface ForwardSubmission {
  /** The account to send from: the composer's From dropdown. */
  from: string;
  /** The `To` recipients, comma-separated. */
  to: string;
  /** The subject line; may be empty. */
  subject: string;
  /** The shared composer document. */
  document: ComposerDocument;
  /** Host-resolved bytes for every attachment handle; empty with no attachments. */
  blobs: ComposerBlob[];
}

/**
 * An immutable snapshot of the composition a reply or forward opens on: the
 * whole prefill (recipients, subject, threading header, quoted original) as
 * one thing, for a composer to hydrate from.
 */
export interface ComposeRequestSnapshot {
  /** Which composition this is: the verbs to render, the submit to send. */
  kind: ComposeKind;
  /** The account to compose from: the original's own account. */
  from: string;
  /** The `To` recipients, comma-joined; empty for a forward. */
  to: string;
  /** The `Cc` recipients, comma-joined; non-empty only for a reply-all. */
  cc: string;
  /** The subject, already prefixed (`Re:`/`Fwd:`). */
  subject: string;
  /** The original's `Message-ID` for a reply to thread under; absent for a forward. */
  in_reply_to?: string;
  /** The composer document the composer opens on: the quoted original, when there was one. */
  document: ComposerDocument;
}

/**
 * The compose family's and its threaded submits' constructors, spread into
 * ipc's `intents`, the family in the enum's own order between the folders
 * and the submits.
 */
export const composeIntents = {
  /** Compose a reply to the original's sender alone. */
  replyTo: (message: MessageRef) => ({ compose: { reply: { message } } }),
  /** Compose a reply to the sender and every other recipient of the original. */
  replyAllTo: (message: MessageRef) => ({ compose: { reply_all: { message } } }),
  /** Compose a forward of the original, its recipients the user's to name. */
  forward: (message: MessageRef) => ({ compose: { forward: { message } } }),
  /** Queue the reply for sending through the durable outbox, under the original. */
  submitRichReply: (reply: ReplySubmission) => ({
    submit_rich_reply: {
      from: reply.from,
      to: reply.to,
      cc: reply.cc,
      in_reply_to: reply.in_reply_to,
      subject: reply.subject,
      document: reply.document,
      blobs: reply.blobs,
    },
  }),
  /** Queue the forward for sending through the durable outbox, answering no message. */
  submitRichForward: (forward: ForwardSubmission) => ({
    submit_rich_forward: {
      from: forward.from,
      to: forward.to,
      subject: forward.subject,
      document: forward.document,
      blobs: forward.blobs,
    },
  }),
};
