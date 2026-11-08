// SPDX-License-Identifier: GPL-3.0-only
// The swipe action table: the four acts the intent surface serves, the
// direction-to-action mapping the settings configure, the intent each act
// dispatches over a row, and the reverse the undo toast dispatches. A move's
// reverse is a move back into the folder the list was showing, expressible
// only when that folder is known and the row names a single message; the
// state flips carry their own flip back.

import { intents, type FolderRef, type Intent } from '@/services/ipc';
import type { MessageRowView } from '@/services/surfaces';

/** Which way the pointer travelled. */
export type SwipeDirection = 'right' | 'left';

/** One act a row swipe can take: the four acts the intent surface serves.
 *  The brief's five-option list carried two names for one act (Trash and
 *  Delete both name the one trash verb the app holds, the row menu's own
 *  Delete), so the select offers each act once. */
export type SwipeAction = 'archive' | 'trash' | 'read' | 'flag';

/** The acts the settings select offers, in its order. */
export const SWIPE_ACTIONS: ReadonlyArray<{ id: SwipeAction; label: string }> = [
  { id: 'archive', label: 'Archive' },
  { id: 'trash', label: 'Trash' },
  { id: 'read', label: 'Read' },
  { id: 'flag', label: 'Flag' },
];

/** How far the pointer must travel before a release fires. */
export const SWIPE_FIRE_PX = 72;

/** How far the row's content may travel with the pointer. */
export const SWIPE_REVEAL_PX = 96;

/** The configured action for one direction. */
export function swipeActionFor(
  direction: SwipeDirection,
  primary: SwipeAction,
  secondary: SwipeAction,
): SwipeAction {
  return direction === 'right' ? primary : secondary;
}

/** The intent one act dispatches over one row: the same intents the row
 *  menu's own verbs dispatch, the read and flag legs flipping what the row
 *  carries. */
export function swipeIntent(action: SwipeAction, row: MessageRowView): Intent {
  switch (action) {
    case 'archive':
      return intents.archiveMessage(row.key);
    case 'trash':
      return intents.moveToTrash(row.key);
    case 'read':
      return intents.markRead(row.key, row.unread);
    case 'flag':
      return intents.setFlagged(row.key, !row.flagged);
    }
}

/** The intent that reverses one act, or null when none is expressible: a
 *  move's reverse files the message back into the folder it sat in, which
 *  needs a known sitting folder and a row that names a message (a move
 *  batch cannot name a conversation); the state flips reverse through
 *  their own flip. */
export function swipeReverse(
  action: SwipeAction,
  row: MessageRowView,
  sittingFolder: FolderRef | null,
): Intent | null {
  if (action === 'archive' || action === 'trash') {
    if (sittingFolder === null || !('message' in row.key)) {
      return null;
    }
    return intents.moveMessages([row.key.message], sittingFolder);
  }
  if (action === 'read') {
    return intents.markRead(row.key, !row.unread);
  }
  return intents.setFlagged(row.key, row.flagged);
}

/** Whether a release at (dx, dy) fires: past the threshold and
 *  horizontally dominant, so a gesture the vertical axis won never fires. */
export function swipeFires(dx: number, dy: number): boolean {
  return Math.abs(dx) >= SWIPE_FIRE_PX && Math.abs(dx) > Math.abs(dy);
}

/** The toast sentence naming one act in the past tense. */
export function swipeToastSentence(action: SwipeAction): string {
  switch (action) {
    case 'archive':
      return 'Archived';
    case 'trash':
      return 'Moved to Trash';
    case 'read':
      return 'Marked as read';
    case 'flag':
      return 'Flagged';
  }
}

/** The reveal's chrome for one act: the colour class the revealed layer
 *  wears, destructive only for the act that discards. */
export function swipeRevealClass(action: SwipeAction): string {
  switch (action) {
    case 'archive':
      return 'bg-primary text-primary-foreground';
    case 'trash':
      return 'bg-destructive text-white';
    case 'read':
      return 'bg-muted text-foreground';
    case 'flag':
      return 'bg-[var(--warning)] text-foreground';
  }
}
