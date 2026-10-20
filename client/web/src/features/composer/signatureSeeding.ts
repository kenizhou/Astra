// SPDX-License-Identifier: GPL-3.0-only
// The signature seeding rules, the web mirror of the core's own
// (astra-app's signatures.rs): the block a composition opens with seeds
// directly beneath the quoted original when the document carries one and
// at the top otherwise, a document already bearing a signature seeds
// nothing, and a From change swaps only a signature that still equals the
// seeded baseline. An edited signature no longer equals the baseline and
// is the person's: it stays.

import type { Block, Signature } from '@/services/ipc';
import { useSettingsStore } from '@/features/settings/settingsStore';

import type { EditorDocument } from './documentWire';

/** Whether two signatures are the same words in both legs. */
function sameSignature(one: Signature, other: Signature): boolean {
  return one.body_html === other.body_html && one.body_plain === other.body_plain;
}

/**
 * Seeds `signature` into the composition's blocks: directly beneath the
 * quoted original when the document carries one, otherwise at the top. A
 * document already bearing a signature seeds nothing, so a resumed draft
 * or a re-opened composition owns the one it has.
 */
export function seedSignatureBlocks(blocks: Block[], signature: Signature): Block[] {
  if (blocks.some((block) => 'Signature' in block)) {
    return blocks;
  }
  const at = blocks.reduce(
    (last, block, index) => ('Quote' in block ? index + 1 : last),
    0,
  );
  return [...blocks.slice(0, at), { Signature: signature }, ...blocks.slice(at)];
}

/**
 * Replaces every retained block that still equals the seeded baseline with
 * `incoming`, or drops it when the new account seeds nothing. A signature
 * the person edited no longer equals the baseline and stays; when no
 * baseline stands, nothing swaps and the document returns as it came.
 */
export function swapSeededSignature(
  editor: EditorDocument,
  seeded: Signature | null,
  incoming: Signature | null,
): EditorDocument {
  if (seeded === null) {
    return editor;
  }
  let changed = false;
  const retained = [];
  for (const kept of editor.retained) {
    if ('Signature' in kept.block && sameSignature(kept.block.Signature, seeded)) {
      changed = true;
      if (incoming !== null) {
        retained.push({ index: kept.index, block: { Signature: incoming } });
      }
      continue;
    }
    retained.push(kept);
  }
  return changed ? { ...editor, retained } : editor;
}

/**
 * The signature `account` seeds a composition with: the reply slot's own
 * when `reply`, else the new-mail slot's, read from the settings
 * snapshot's account rows. Null seeds nothing.
 */
export function signatureForAccount(account: string, reply: boolean): Signature | null {
  const rows = useSettingsStore.getState().accounts;
  const assignment = rows.find((row) => row.account === account)?.signatures;
  const signature = reply ? assignment?.reply : assignment?.new_mail;
  return signature ?? null;
}
