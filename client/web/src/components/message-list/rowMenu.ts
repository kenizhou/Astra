// SPDX-License-Identifier: GPL-3.0-only
// The message row's context menu builder: pure functions from the target
// rows to the item list, ported from kylins' thread menu in the order the
// user pinned. Working items dispatch the intents the host already
// accepts (batch targets loop the per-row intents, the multi-select's own
// compromise until the host takes a batch); items the host cannot serve
// yet stand as named placeholders.

import {
  ArchiveIcon,
  FlagIcon,
  ForwardIcon,
  MarkReadIcon,
  ReplyAllIcon,
  ReplyIcon,
  TrashIcon,
} from '@/components/icons';
import { placeholderItem, type ContextMenuItem } from '@/components/ui/ContextMenu';
import { dispatch, intents, type MessageRef } from '@/services/ipc';
import type { FolderRow, MessageRowView, ViewMode } from '@/services/surfaces';

/** The account the move-to picker offers: the selection's own account, the
 *  folders the pane holds for it, and the folder the rows sit in. */
export interface MoveToContext {
  account: string;
  currentKey: string | null;
  folders: FolderRow[];
}

/** Fires one intent per target row, fire-and-forget: the snapshots carry
 *  the outcomes back. */
function forEach(targets: MessageRowView[], act: (row: MessageRowView) => void): void {
  for (const row of targets) {
    act(row);
  }
}

/** The one target the compose verbs and the copy pair accept. */
function single(targets: MessageRowView[]): MessageRowView | null {
  return targets.length === 1 ? targets[0]! : null;
}

/** The message half of a single message-keyed target, when it is one. */
function singleMessage(targets: MessageRowView[]): MessageRef | null {
  if (targets.length !== 1) {
    return null;
  }
  const key = targets[0]!.key;
  return 'message' in key ? key.message : null;
}

/** Copy to the clipboard; a no-op where the webview withholds the API. */
function copy(text: string): void {
  void navigator.clipboard?.writeText(text);
}

/** The move-to item: the account's other folders as a flyout, each choice
 *  moving the whole target set in one batch. Hidden while the targets span
 *  accounts (moves stay within one) or the account holds no other folder,
 *  disabled with the conversation reason while a target is a conversation,
 *  which a move batch cannot name. */
function moveItem(targets: MessageRowView[], context: MoveToContext | undefined): ContextMenuItem | null {
  if (context === undefined) {
    return null;
  }
  const oneAccount = targets.every((row) => row.account_id === context.account);
  if (!oneAccount) {
    return null;
  }
  const destinations = context.folders.filter((folder) => folder.key !== context.currentKey);
  if (destinations.length === 0) {
    return null;
  }
  if (targets.some((row) => 'thread' in row.key)) {
    const reason = 'A whole conversation cannot be moved for this yet';
    return { label: 'Move to folder', disabled: true, title: reason, description: reason };
  }
  const refs: MessageRef[] = messageRefs(targets);
  return {
    label: 'Move to folder',
    children: destinations.map((folder) => ({
      label: folder.name,
      onSelect: () => void dispatch(intents.moveMessages(refs, { account: context.account, key: folder.key })),
    })),
  };
}

/** The message half of every target: the batch the move intent names. */
function messageRefs(targets: MessageRowView[]): MessageRef[] {
  return targets.flatMap((row) => ('message' in row.key ? [row.key.message] : []));
}

/** The menu one message row (or a batch of them) opens. */
export function buildRowMenu(
  targets: MessageRowView[],
  viewMode: ViewMode,
  moveTo: MoveToContext | undefined = undefined,
): ContextMenuItem[] {
  const one = single(targets);
  const oneMessage = singleMessage(targets);
  const composeReason =
    oneMessage !== null
      ? undefined
      : one === null
        ? 'Pick one message for this'
        : 'A whole conversation cannot be named for this yet';
  const copyReason = 'Pick one message to copy from';
  const multi = targets.length > 1;
  const noun = viewMode === 'threaded' ? 'conversations' : 'messages';
  const anyUnread = targets.some((row) => row.unread);
  const allFlagged = targets.every((row) => row.flagged);
  const move = moveItem(targets, moveTo);

  return [
    placeholderItem('Open in New Window', 'Opening a message in its own window arrives with a future slice'),
    { separator: true },
    {
      label: 'Reply',
      icon: ReplyIcon,
      disabled: composeReason !== undefined,
      title: composeReason,
      description: composeReason,
      onSelect: () => void dispatch(intents.replyTo(oneMessage!)),
    },
    {
      label: 'Reply All',
      icon: ReplyAllIcon,
      disabled: composeReason !== undefined,
      title: composeReason,
      description: composeReason,
      onSelect: () => void dispatch(intents.replyAllTo(oneMessage!)),
    },
    {
      label: 'Forward',
      icon: ForwardIcon,
      disabled: composeReason !== undefined,
      title: composeReason,
      description: composeReason,
      onSelect: () => void dispatch(intents.forward(oneMessage!)),
    },
    {
      label: anyUnread ? 'Mark as Read' : 'Mark as Unread',
      icon: MarkReadIcon,
      onSelect: () => forEach(targets, (row) => void dispatch(intents.markRead(row.key, row.unread))),
    },
    {
      label: allFlagged ? 'Unflag' : 'Flag',
      icon: FlagIcon,
      onSelect: () => forEach(targets, (row) => void dispatch(intents.setFlagged(row.key, !allFlagged))),
    },
    { separator: true },
    ...(move !== null ? [move] : []),
    placeholderItem('Junk', 'Junk reporting arrives with a future slice'),
    { separator: true },
    {
      label: multi ? `Delete ${targets.length} ${noun}` : 'Delete',
      icon: TrashIcon,
      danger: true,
      onSelect: () => forEach(targets, (row) => void dispatch(intents.moveToTrash(row.key))),
    },
    {
      label: multi ? `Archive ${targets.length} ${noun}` : 'Archive',
      icon: ArchiveIcon,
      onSelect: () => forEach(targets, (row) => void dispatch(intents.archiveMessage(row.key))),
    },
    { separator: true },
    {
      label: 'Copy Subject',
      disabled: one === null,
      title: one === null ? copyReason : undefined,
      description: one === null ? copyReason : undefined,
      onSelect: () => copy(one!.subject),
    },
    {
      label: 'Copy Sender',
      disabled: one === null,
      title: one === null ? copyReason : undefined,
      description: one === null ? copyReason : undefined,
      onSelect: () => copy(one!.sender_address),
    },
  ];
}

/** The menu a saved-draft row opens; every action is a placeholder until
 *  the host resumes and deletes drafts by key. */
export function buildDraftMenu(): ContextMenuItem[] {
  return [
    placeholderItem(
      'Open',
      'Opening an individual draft arrives with a future slice; the app start resume opens the latest',
    ),
    placeholderItem('Open in New Window', 'Opening a draft in its own window arrives with a future slice'),
    { separator: true },
    { ...placeholderItem('Delete Draft', 'Deleting a draft arrives with a future slice'), danger: true },
  ];
}
