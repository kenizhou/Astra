// SPDX-License-Identifier: GPL-3.0-only
// The message row's context menu builder: pure functions from the target
// rows to the item list, ported from kylins' thread menu in the order the
// user pinned. Working items dispatch the intents the host already accepts
// (batch targets loop the per-row intents, the multi-select's own
// compromise until the host takes a batch); the own-window verb opens the
// pop-out reading window for a single message; the spam report rides the
// report family's mark_junk and stands only where the account's provider
// can report; items the host cannot serve yet stand as named placeholders.

import {
  ArchiveIcon,
  FlagIcon,
  ForwardIcon,
  JunkIcon,
  MarkReadIcon,
  PopOutIcon,
  ReplyAllIcon,
  ReplyIcon,
  TrashIcon,
} from '@/components/icons';
import { placeholderItem, type ContextMenuItem } from '@/components/ui/ContextMenu';
import { dispatch, intents, type MessageRef } from '@/services/ipc';
import { openMessageWindow } from '@/features/reader/readerWindows';
import type { FolderRow, MessageRowView, ViewMode } from '@/services/surfaces';

/** The account context the menu holds: the account the picker and the
 *  report gate ask about, its folders, the folder the rows sit in, and
 *  whether its provider can report mail. */
export interface RowAccountContext {
  account: string;
  currentKey: string | null;
  canReportMail: boolean;
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

/** The own-window verb: opens the pop-out reading window over the single
 *  message, titled by its subject, carrying the folder the list sits in so
 *  the window can judge the junk-folder context its Not-spam verb needs.
 *  A thread row or a batch cannot name one message, so the verb stands
 *  disabled with the reason. */
function openWindowItem(
  one: MessageRowView | null,
  oneMessage: MessageRef | null,
  context: RowAccountContext | undefined,
): ContextMenuItem {
  const reason =
    oneMessage !== null
      ? undefined
      : one === null
        ? 'Pick one message for this'
        : 'A whole conversation cannot be opened in a window for this yet';
  return {
    label: 'Open in New Window',
    icon: PopOutIcon,
    disabled: reason !== undefined ? true : undefined,
    title: reason,
    description: reason,
    onSelect: () => {
      if (one === null) {
        return;
      }
      void openMessageWindow(
        oneMessage!,
        one.subject === '' ? '(no subject)' : one.subject,
        context?.currentKey ?? null,
      );
    },
  };
}

/** The folder names the junk destination answers to besides its role,
 *  mirroring the core resolver's own candidates: an account whose server
 *  tags no Junk role still files reports when a folder carries one of
 *  these names. */
const JUNK_FOLDER_NAMES: readonly string[] = [
  'junk',
  'junk mail',
  'junk e-mail',
  'spam',
  'bulk mail',
  'ongewenste e-mail',
];

/** The spam-report verb: one mark_junk per target through the report
 *  family, the report that files the message into Junk as part of itself.
 *  Offered only where the account's provider can report its mail AND the
 *  tree resolves a junk destination (the RFC 6154 role or a conventional
 *  name), so the verb never stands where dispatching it would only be
 *  refused or silently skipped. */
function spamItem(
  targets: MessageRowView[],
  context: RowAccountContext | undefined,
): ContextMenuItem | null {
  if (context === undefined || !context.canReportMail) {
    return null;
  }
  const junk = context.folders.some(
    (folder) =>
      folder.role === 'junk' ||
      JUNK_FOLDER_NAMES.includes(folder.name.trim().toLowerCase()),
  );
  if (!junk) {
    return null;
  }
  return {
    label: 'Mark as spam',
    icon: JunkIcon,
    onSelect: () => forEach(targets, (row) => void dispatch(intents.markJunk(row.key))),
  };
}

/** The move-to item: the account's other folders as a flyout, each choice
 * moving the whole target set in one batch. Hidden while the targets span
 * accounts (moves stay within one) or the account holds no other folder,
 * disabled with the conversation reason while a target is a conversation,
 * which a move batch cannot name. */
function moveItem(targets: MessageRowView[], context: RowAccountContext | undefined): ContextMenuItem | null {
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
  context: RowAccountContext | undefined = undefined,
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
  const move = moveItem(targets, context);
  const spam = spamItem(targets, context);

  return [
    openWindowItem(one, oneMessage, context),
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
    ...(spam !== null ? [spam] : []),
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
