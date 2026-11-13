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

/**
 * One imported key as the host's public view answers it: no private material
 * crosses, only the `hasPrivate` flag saying a soft private blob exists at
 * rest. The fields mirror the desktop host's camelCase serde exactly.
 */
export interface CryptoKeyRow {
  id: string;
  accountId: string;
  /** 'openpgp' | 'smime' | 'sm'. */
  standard: string;
  /** 'public' | 'private' | 'cert' | 'intermediate'. */
  keyType: string;
  email?: string;
  fingerprint: string;
  /** How the key arrived ('imported', 'generated', 'harvested', ...). */
  origin: string;
  isDefaultSign: boolean;
  isDefaultEncrypt: boolean;
  createdAt: string;
  expiresAt?: string;
  subjectCn?: string;
  notBefore?: string;
  hasPrivate: boolean;
  tokenSerial?: string;
  tokenKeyId?: string;
}

/**
 * Imports key material into the account: the host dispatches public material
 * to the public-only store path and secret material to the passphrase-gated
 * private path, answering the landed row. The bytes cross into host memory;
 * the optional passphrase stays in-process, never logged or persisted.
 */
export function importKey(
  account: string,
  bytes: Uint8Array,
  passphrase?: string,
): Promise<CryptoKeyRow> {
  return invoke('import_key', {
    account,
    bytes: Array.from(bytes),
    passphrase: passphrase ?? null,
  });
}

/**
 * Lists the account's imported keys across both standards, public view only.
 */
export function listKeys(account: string): Promise<CryptoKeyRow[]> {
  return invoke('list_keys', { account });
}

/**
 * One staged collected key as the host's review list answers it: the public
 * material a discovery source (Autocrypt, gossip, WKD, ...) observed but the
 * person has not yet accepted. The fields mirror the desktop host's camelCase
 * serde exactly; `publicData` is the base64 key bytes, `seenAt` a unix-seconds
 * string. No secret crosses this surface.
 */
export interface CollectedKeyRow {
  id: number;
  accountId?: string;
  peerEmail?: string;
  /** 'openpgp' | 'smime' | 'sm'. */
  standard?: string;
  fingerprint?: string;
  /** The raw key bytes as base64, when present. */
  publicData?: string;
  /** The discovery source ('autocrypt', 'wkd', 'harvested', ...). */
  source?: string;
  seenAt: string;
}

/**
 * Lists the account's staged collected keys across both standards, newest
 * first: the review surface's list. Public material only.
 */
export function listCollected(account: string): Promise<CollectedKeyRow[]> {
  return invoke('list_collected', { account });
}

/**
 * Accepts a staged collected key into the keystore paired with the trust
 * decision that makes it send-usable: `verified` true records the out-of-band
 * verification (an upgrade), false accepts without verification (never
 * downgrading a standing decision). The email binds the acceptance, or null
 * when the key carries no peer address. Answers the landed row (public view).
 */
export function promoteCollected(
  account: string,
  standard: string,
  fingerprint: string,
  email: string | null,
  verified: boolean,
): Promise<CryptoKeyRow> {
  return invoke('promote_collected', { account, standard, fingerprint, email, verified });
}

/**
 * Rejects a staged collected key by its staging id: removes the staging row
 * and records a rejected decision. Staging is decision-blind, so a re-opened
 * source re-stages and the list re-offers it; the rejection stands (an
 * unverified promote never downgrades it; the resolver answers Rejected once a
 * keystore row exists). The email binds the decision, or null when absent.
 */
export function rejectCollected(
  account: string,
  id: number,
  standard: string,
  fingerprint: string,
  email: string | null,
): Promise<void> {
  return invoke('reject_collected', { account, id, standard, fingerprint, email });
}