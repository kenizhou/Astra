// SPDX-License-Identifier: GPL-3.0-only
// The message row's context menu builder: pure functions from the target
// rows to the item list, ported from kylins' thread menu in the order the
// user pinned. Working items dispatch the intents the host already accepts
// (batch targets loop the per-row intents, the multi-select's own
// compromise until the host takes a batch); the own-window verb opens the
// pop-out reading window for a single message; the spam report rides the
// report family's mark_junk and stands only where the account's provider
// can report; items the host cannot serve yet stand as named placeholders.
// The pin and VIP entries are the exceptions: pure view state over the
// local sets, no intent, the batch landing one state.

import {
  ArchiveIcon,
  BellIcon,
  CleanIcon,
  ComposeIcon,
  FlagIcon,
  ForwardIcon,
  JunkIcon,
  MarkReadIcon,
  MoonIcon,
  PinIcon,
  PopOutIcon,
  ReplyAllIcon,
  ReplyIcon,
  StarIcon,
  TrashIcon,
} from '@/components/icons';
import { type ContextMenuItem } from '@/components/ui/ContextMenu';
import { dispatch, intents, type MessageRef } from '@/services/ipc';
import { openMessageWindow } from '@/features/reader/readerWindows';
import { openDraftRow, openDraftRowWindow } from '@/features/composer/draftOpen';
import { usePinnedRowsStore } from '@/features/list-pin/pinnedRowsStore';
import { useVipSendersStore } from '@/features/vip/vipSendersStore';
import { blockSenderItem } from '@/features/junk-blocking/blockSenderMenu';
import { categorizeItem } from '@/features/categories/categoryMenu';
import { watchItem } from '@/features/watch-replies/watchMenu';
import { useSweepStore } from '@/features/sweep/sweepStore';
import { useSnoozeStore } from '@/features/snooze/snoozeStore';
import { useReminderStore } from '@/features/reminders/reminderStore';
import { rowKeyId } from '@/components/message-list/rowIdentity';
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

/** The pin verb: pure view state over the local pin set, no intent. The
 *  label flips on the targets' pin state (Unpin only when every target is
 *  pinned), and the batch lands one state: Unpin clears every target, Pin
 *  pins the unpinned ones and leaves the already pinned alone. */
function pinItem(targets: MessageRowView[]): ContextMenuItem {
  const allPinned = targets.every((row) => usePinnedRowsStore.getState().isPinned(rowKeyId(row.key)));
  const pin = !allPinned;
  return {
    label: allPinned ? 'Unpin' : 'Pin',
    icon: PinIcon,
    onSelect: () =>
      forEach(targets, (row) => {
        const store = usePinnedRowsStore.getState();
        const id = rowKeyId(row.key);
        if (store.isPinned(id) !== pin) {
          store.toggle(id);
        }
      }),
  };
}

/** The VIP verb: pure view state over the local sender set, no intent.
 *  The label flips on the targets' senders' state (Remove VIP only when
 *  every target's sender stands VIP), and the batch lands one state:
 *  Remove VIP clears every target's sender, Add VIP marks the unmarked
 *  ones and leaves the already marked alone. The set keys by address, so
 *  one mark stands across accounts; a blank sender is refused by the
 *  store and stays unmarked. */
function vipItem(targets: MessageRowView[]): ContextMenuItem {
  const allVip = targets.every((row) => useVipSendersStore.getState().has(row.sender_address));
  const vip = !allVip;
  return {
    label: allVip ? 'Remove VIP' : 'Add VIP',
    icon: StarIcon,
    onSelect: () =>
      forEach(targets, (row) => {
        const store = useVipSendersStore.getState();
        if (store.has(row.sender_address) !== vip) {
          store.toggle(row.sender_address);
        }
      }),
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

/** Whether the account can report mail as junk: the provider can report AND
 *  the tree resolves a junk destination (the RFC 6154 role or a conventional
 *  name). The spam verb's visibility rule, shared by the block dialog's
 *  optional report leg so the two never disagree. */
function junkReportable(context: RowAccountContext | undefined): boolean {
  if (context === undefined || !context.canReportMail) {
    return false;
  }
  return context.folders.some(
    (folder) =>
      folder.role === 'junk' ||
      JUNK_FOLDER_NAMES.includes(folder.name.trim().toLowerCase()),
  );
}

/** The spam-report verb: one mark_junk per target through the report
 *  family, the report that files the message into Junk as part of itself.
 *  Offered only where the account can report junk, so the verb never stands
 *  where dispatching it would only be refused or silently skipped. */
function spamItem(
  targets: MessageRowView[],
  context: RowAccountContext | undefined,
): ContextMenuItem | null {
  if (!junkReportable(context)) {
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

/** The permanent delete: the irreversible verb behind its own confirm, the
 *  sentence the Trash folder delete's own confirm shaped. Stands anywhere
 *  the menu opens, danger-styled, below the ordinary Delete (which stays
 *  the move-to-trash), and only ever names one row: the verb never rides a
 *  batch loop, so a multi-selection stands disabled with the reason. */
function permanentDeleteItem(targets: MessageRowView[]): ContextMenuItem {
  const reason = targets.length > 1 ? 'Pick one row for this' : undefined;
  const noun = targets.length === 1 && 'thread' in targets[0]!.key ? 'conversation' : 'message';
  return {
    label: 'Delete Permanently',
    icon: TrashIcon,
    danger: true,
    disabled: reason !== undefined ? true : undefined,
    title: reason,
    description: reason,
    onSelect: () => {
      if (reason !== undefined) {
        return;
      }
      const question = `Permanently delete this ${noun}? This cannot be undone.`;
      if (!window.confirm(question)) {
        return;
      }
      void dispatch(intents.deleteMessage(targets[0]!.key));
    },
  };
}

/** The sweep entry: opens the clean-up dialog over the one row's sender,
 *  the dialog previewing that sender's loaded rows in the sitting folder
 *  before a confirmed batch. A batch cannot name one sender, so the entry
 *  stands disabled with the reason. */
function sweepItem(targets: MessageRowView[]): ContextMenuItem {
  const one = single(targets);
  const reason = one === null ? 'Pick one row for this' : undefined;
  return {
    label: 'Clean up sender…',
    icon: CleanIcon,
    disabled: reason !== undefined ? true : undefined,
    title: reason,
    description: reason,
    onSelect: () => {
      if (one !== null) {
        useSweepStore.getState().open(one.sender_address);
      }
    },
  };
}

/** The snooze entry: opens the wake-time picker over the one message, the
 *  dialog setting it aside until the chosen instant. A conversation or a
 *  batch cannot name one message, so the entry stands disabled with the
 *  reason (a move names messages only). */
function snoozeItem(targets: MessageRowView[]): ContextMenuItem {
  const oneMessage = singleMessage(targets);
  const reason =
    oneMessage !== null
      ? undefined
      : targets.length === 1
        ? 'A whole conversation cannot be snoozed for this yet'
        : 'Pick one message for this';
  return {
    label: 'Snooze…',
    icon: MoonIcon,
    disabled: reason !== undefined ? true : undefined,
    title: reason,
    description: reason,
    onSelect: () => {
      if (oneMessage !== null) {
        useSnoozeStore.getState().open(oneMessage);
      }
    },
  };
}

/** The remind entry: opens the wake-time picker over the one message, the
 *  dialog asking to be notified about it at the chosen instant (the message
 *  never moves or flags). A conversation or a batch cannot name one message,
 *  so the entry stands disabled with the reason. */
function remindItem(targets: MessageRowView[]): ContextMenuItem {
  const oneMessage = singleMessage(targets);
  const reason =
    oneMessage !== null
      ? undefined
      : targets.length === 1
        ? 'A whole conversation cannot be reminded for this yet'
        : 'Pick one message for this';
  return {
    label: 'Remind me…',
    icon: BellIcon,
    disabled: reason !== undefined ? true : undefined,
    title: reason,
    description: reason,
    onSelect: () => {
      if (oneMessage !== null) {
        useReminderStore.getState().open(oneMessage);
      }
    },
  };
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
    pinItem(targets),
    vipItem(targets),
    blockSenderItem(targets, junkReportable(context)),
    categorizeItem(targets),
    watchItem(targets),
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
    permanentDeleteItem(targets),
    {
      label: multi ? `Archive ${targets.length} ${noun}` : 'Archive',
      icon: ArchiveIcon,
      onSelect: () => forEach(targets, (row) => void dispatch(intents.archiveMessage(row.key))),
    },
    sweepItem(targets),
    snoozeItem(targets),
    remindItem(targets),
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

/** The menu a saved-draft row opens: the row's own three verbs over its key,
 *  the resume into the composer (the pane, or its own window), and the
 *  discard of the stored copy. A batch cannot name one draft, so every verb
 *  stands disabled with the reason. */
export function buildDraftMenu(targets: MessageRowView[]): ContextMenuItem[] {
  const one = single(targets);
  const oneMessage = singleMessage(targets);
  const reason = oneMessage !== null ? undefined : 'Pick one draft for this';
  const disabled = reason !== undefined;
  const guard = (): MessageRef | null => (reason === undefined ? oneMessage : null);
  return [
    {
      label: 'Open',
      icon: ComposeIcon,
      disabled: disabled || undefined,
      title: reason,
      description: reason,
      onSelect: () => {
        const message = guard();
        if (message !== null) {
          void openDraftRow(message);
        }
      },
    },
    {
      label: 'Open in New Window',
      icon: PopOutIcon,
      disabled: disabled || undefined,
      title: reason,
      description: reason,
      onSelect: () => {
        const message = guard();
        if (message !== null) {
          void openDraftRowWindow(message, one?.subject ?? '');
        }
      },
    },
    { separator: true },
    {
      label: 'Delete Draft',
      icon: TrashIcon,
      danger: true,
      disabled: disabled || undefined,
      title: reason,
      description: reason,
      onSelect: () => {
        const message = guard();
        if (message !== null) {
          void dispatch(intents.discardStored(message));
        }
      },
    },
  ];
}
