// SPDX-License-Identifier: GPL-3.0-only
// The sending half of the IPC wire: the submit intent's payload, the outbox
// family, and the queue's snapshot types, with their constructors. Split from
// ipc.ts to keep that file under the line cap, and re-exported whole through
// it, so ipc stays the one import point of the wire. The literals are pinned
// in sending.test.ts against the JSON astra-app's protocol_tests.rs and
// astra-viewmodel's sending_tests.rs assert.

import type { ComposerDocument } from './composerDocument';

/** Host-resolved bytes for one composer blob handle, riding beside the document at submit. */
export interface ComposerBlob {
  /** The opaque blob handle the document's attachment named. */
  handle: string;
  /** The attachment bytes for that handle. */
  bytes: number[];
}

/** The fields a submit carries, beside the composer document itself. */
export interface MailSubmission {
  /** The account to send from: the composer's From dropdown. */
  from: string;
  /** The `To` recipients, comma-separated. */
  to: string;
  /** The `Cc` recipients, comma-separated; may be empty. */
  cc: string;
  /** The `Bcc` recipients, comma-separated; may be empty. */
  bcc: string;
  /** The subject line; may be empty. */
  subject: string;
  /** The shared composer document. */
  document: ComposerDocument;
  /** Host-resolved bytes for every attachment handle; empty with no attachments. */
  blobs: ComposerBlob[];
}

/** A queued send an outbox action names: its account and durable op id together. */
export interface QueuedRef {
  /** The account whose outbox holds the send. */
  account: string;
  /** The durable op's id, as the store assigned it. */
  op: number;
}

/** What the outbox panel asks the runtime to do, nested under the `outbox` tag. */
export type OutboxIntent =
  | 'show'
  | { cancel: { queued: QueuedRef } }
  | { send_now: { queued: QueuedRef } }
  | { confirm_sent: { queued: QueuedRef } }
  | { confirm_not_sent: { queued: QueuedRef } }
  | { answer_gate: { hold: string; send_anyway: boolean } }
  | { retry_held: { hold: string } }
  | { withdraw_held: { hold: string } };

/** Where a queued send has got to; crosses as a plain snake_case word. */
export type QueuedState = 'queued' | 'sending' | 'unconfirmed' | 'failed';

/** The identity of one queued send: its account and outbox op id together. */
export interface QueuedKey {
  /** The account whose outbox holds the send. */
  account: string;
  /** The durable op's id, as the store assigned it. */
  op: number;
}

/** One queued send, as the outbox panel shows it. */
export interface QueuedRow {
  /** The send's identity: what an action names to act on this row. */
  queued: QueuedKey;
  /** Where the send has got to. */
  state: QueuedState;
  /** The subject line, empty if the draft carried none. */
  subject: string;
  /** When the send joined the queue, as a whole-second UTC `…Z` instant; empty when unknown. */
  arrived_at: string;
}

/** The sends that have not gone out, across every account; empty when every send has gone. */
export interface SendingSnapshot {
  /** The outstanding sends, oldest queue position first within each account. */
  rows: QueuedRow[];
  /** The sends the plugin gate is holding, in hold order; absent when none. */
  held?: HeldSend[];
}

/** Where a held send stands; crosses as a plain snake_case word. */
export type HeldState = 'checking' | 'needs_answer' | 'blocked';

/** One send the plugin gate is holding back from the queue. */
export interface HeldSend {
  /** The hold's id: what an answer, a retry, or a withdraw names. */
  hold: string;
  /** The account the send would leave from. */
  account: string;
  /** The subject line, empty if the draft carried none. */
  subject: string;
  /** The plugin whose verdict is awaited or whose refusal holds the send. */
  plugin: string;
  /** The reason, in product words. */
  reason: string;
  /** Where the hold stands. */
  state: HeldState;
}

/** The sending constructors, spread into ipc's `intents` in the enum's own order. */
export const sendingIntents = {
  /** Queue the composer's rich document for sending through the durable outbox. */
  submitRichMail: (mail: MailSubmission) => ({
    submit_rich_mail: {
      from: mail.from,
      to: mail.to,
      cc: mail.cc,
      bcc: mail.bcc,
      subject: mail.subject,
      document: mail.document,
      blobs: mail.blobs,
    },
  }),
  /** Show the sends that have not gone out, across every account. */
  showOutbox: () => ({ outbox: 'show' as const }),
  /** Withdraw a queued send so it is never delivered, or dismiss a refused one. */
  cancelQueuedSend: (queued: QueuedRef) => ({ outbox: { cancel: { queued } } }),
  /** Attempt a queued send now rather than waiting out its backoff. */
  sendQueuedNow: (queued: QueuedRef) => ({ outbox: { send_now: { queued } } }),
  /** Answer an unconfirmed send: it reached its recipients, so it settles. */
  confirmQueuedSent: (queued: QueuedRef) => ({ outbox: { confirm_sent: { queued } } }),
  /** Answer an unconfirmed send: it did not reach them, so it is sent again. */
  confirmQueuedNotSent: (queued: QueuedRef) => ({
    outbox: { confirm_not_sent: { queued } },
  }),
  /** Answer a prompt-user gate: Send Anyway queues without re-gating, Deny keeps the draft. */
  answerGate: (hold: string, sendAnyway: boolean) => ({
    outbox: { answer_gate: { hold, send_anyway: sendAnyway } },
  }),
  /** Re-run the gate over a held send. */
  retryHeld: (hold: string) => ({ outbox: { retry_held: { hold } } }),
  /** Drop a held send; the draft behind it stays in Drafts. */
  withdrawHeld: (hold: string) => ({ outbox: { withdraw_held: { hold } } }),
};
