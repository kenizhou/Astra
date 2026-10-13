// SPDX-License-Identifier: GPL-3.0-only
// The ribbon's read mode: New Email opens a plain composition the way the
// floating action does, Reply, Reply all, and Forward dispatch the compose
// family for the message the reading pane has open, and Mark read/unread
// and Flag/unflag act on the list's selected row, their labels naming the
// state the click leaves behind. Without an open message or a selection the
// matching verbs stand disabled: there is nothing to compose from or act
// on, and the core would have nothing to prefill.

import type { ComponentType } from 'react';

import {
  ComposeIcon,
  FlagIcon,
  ForwardIcon,
  MarkReadIcon,
  ReplyAllIcon,
  ReplyIcon,
  type IconProps,
} from '@/components/icons';
import { sameRowKey } from '@/components/message-list/rowIdentity';
import { dispatch, intents, type Intent, type MessageRef } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { useComposerStore } from '@/features/composer/composerStore';

import { RibbonButton, RibbonGroup } from './RibbonPrimitives';

/** Opens a plain composition from the first account, as the FAB does. */
function openNewMessage(): void {
  const account = useAccountStore.getState().accounts[0];
  if (account === undefined) {
    return;
  }
  useComposerStore.getState().newMessage({ id: account.accountId, label: account.displayName });
}

/** The compose verbs, in display order, each with the intent it dispatches. */
const COMPOSE_ACTIONS: ReadonlyArray<{
  label: string;
  icon: ComponentType<IconProps>;
  verb: (message: MessageRef) => Intent;
}> = [
  { label: 'Reply', icon: ReplyIcon, verb: intents.replyTo },
  { label: 'Reply all', icon: ReplyAllIcon, verb: intents.replyAllTo },
  { label: 'Forward', icon: ForwardIcon, verb: intents.forward },
];

export function ReadRibbon({ iconOnly }: { iconOnly: boolean }) {
  const accountCount = useAccountStore((s) => s.accounts.length);
  const snapshot = useReadingStore((s) => s.snapshot);
  const rows = useListStore((s) => s.rows);
  const selectedKey = useListStore((s) => s.selectedKey);
  const selectedRow =
    selectedKey === null ? undefined : rows.find((row) => sameRowKey(row.key, selectedKey));

  const message = snapshot?.message ?? null;

  return (
    <>
      <RibbonGroup>
        <RibbonButton
          label="New Email"
          icon={ComposeIcon}
          primary
          disabled={accountCount === 0}
          title={accountCount === 0 ? 'Add an account first' : 'New Email'}
          iconOnly={iconOnly}
          onClick={openNewMessage}
        />
      </RibbonGroup>
      <RibbonGroup>
        {COMPOSE_ACTIONS.map((action) => (
          <RibbonButton
            key={action.label}
            label={action.label}
            icon={action.icon}
            disabled={message === null}
            iconOnly={iconOnly}
            onClick={() => {
              if (message !== null) {
                void dispatch(action.verb(message));
              }
            }}
          />
        ))}
      </RibbonGroup>
      <RibbonGroup>
        <RibbonButton
          label={selectedRow?.unread ? 'Mark read' : 'Mark unread'}
          icon={MarkReadIcon}
          disabled={selectedRow === undefined}
          iconOnly={iconOnly}
          onClick={() => {
            if (selectedRow !== undefined) {
              void dispatch(intents.markRead(selectedRow.key, selectedRow.unread));
            }
          }}
        />
        <RibbonButton
          label={selectedRow?.flagged ? 'Unflag' : 'Flag'}
          icon={FlagIcon}
          disabled={selectedRow === undefined}
          iconOnly={iconOnly}
          onClick={() => {
            if (selectedRow !== undefined) {
              void dispatch(intents.setFlagged(selectedRow.key, !selectedRow.flagged));
            }
          }}
        />
      </RibbonGroup>
    </>
  );
}
