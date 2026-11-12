// SPDX-License-Identifier: GPL-3.0-only
// The palette registry: every command maps to an action the shell already
// ships, built from the standing store getters and the existing intents. Each
// run resolves its target at call time; a command whose target is absent
// stands disabled with the reason, never a silent no-op. The palette adds no
// wire type and no intent, and send is never a command: sending rides the
// composer's own submit path, human-only.

import type { ComponentType } from 'react';

import {
  AgentIcon,
  AiIcon,
  ArchiveIcon,
  BulletListIcon,
  CalendarIcon,
  ComposeIcon,
  DraftsIcon,
  FlagIcon,
  ForwardIcon,
  InboxIcon,
  MailIcon,
  MarkReadIcon,
  MoonIcon,
  OrderedListIcon,
  PeopleIcon,
  PlusIcon,
  ReadingIcon,
  RefreshIcon,
  ReplyAllIcon,
  ReplyIcon,
  SearchIcon,
  SentIcon,
  SettingsIcon,
  TrashIcon,
  type IconProps,
} from '@/components/icons';
import { openSearchIsland } from '@/components/layout/searchIsland';
import { sameRowKey } from '@/components/message-list/rowIdentity';
import { openNewMessageComposer } from '@/features/composer/composeWindow';
import { useAppearanceStore } from '@/services/appearance';
import { dispatch, intents, type FolderRef, type MessageRef } from '@/services/ipc';
import type { AccountFolderRow, FolderRole, MessageRowView } from '@/services/surfaces';
import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { carriedAttachmentIds, useReadingStore } from '@/stores/readingStore';
import { nextThemeMode, useUiStore, type ShellView } from '@/stores/uiStore';

/** The grouping a command belongs to; the flat list carries it as metadata. */
export type CommandCategory = 'Go' | 'Compose' | 'Mail' | 'Search' | 'App' | 'View';

/** One palette command: the label the filter matches, the act it runs, and
 *  the reason it stands disabled when its target is absent. */
export interface PaletteCommand {
  id: string;
  label: string;
  category: CommandCategory;
  icon: ComponentType<IconProps>;
  run: () => void;
  disabled?: string;
}

const NO_ACCOUNT = 'Add an account first';
const NO_MESSAGE = 'Open a message first';
const NO_ROW = 'Select a message first';

/** The account the Go folder commands name: the selected folder's account,
 *  else the standing scope's, else the first account. */
function standingAccount(): AccountFolderRow | undefined {
  const { accounts, selected, scope } = useFolderStore.getState();
  const id =
    selected?.account ??
    (scope !== null && scope.kind !== 'all_inboxes' ? scope.account : undefined);
  return accounts.find((account) => account.account_id === id) ?? accounts[0];
}

/** The role folder of the standing account, or the reason none stands. */
function resolveFolder(role: FolderRole, name: string): { ref: FolderRef | null; reason?: string } {
  const account = standingAccount();
  if (account === undefined) {
    return { ref: null, reason: NO_ACCOUNT };
  }
  const folder = account.folders.find((row) => row.role === role);
  if (folder === undefined) {
    return { ref: null, reason: `This account has no ${name} folder` };
  }
  return { ref: { account: account.account_id, key: folder.key } };
}

/** The row the mail commands act on: the local selection resolved against the
 *  standing rows, so a key whose row has gone names nothing. */
function selectedRow(): MessageRowView | undefined {
  const { rows, selectedKey } = useListStore.getState();
  if (selectedKey === null) {
    return undefined;
  }
  return rows.find((row) => sameRowKey(row.key, selectedKey));
}

/** One Go folder command: the select plus selectFolder pair the tree uses. */
function folderCommand(
  id: string,
  label: string,
  role: FolderRole,
  name: string,
  icon: ComponentType<IconProps>,
): PaletteCommand {
  return {
    id,
    label,
    category: 'Go',
    icon,
    disabled: resolveFolder(role, name).reason,
    run: () => {
      const target = resolveFolder(role, name).ref;
      if (target === null) {
        return;
      }
      useFolderStore.getState().select(target);
      void dispatch(intents.selectFolder(target));
    },
  };
}

/** One Go view command: the shell's local view setter, no intent. */
function viewCommand(
  id: string,
  label: string,
  view: ShellView,
  icon: ComponentType<IconProps>,
): PaletteCommand {
  return { id, label, category: 'Go', icon, run: () => useUiStore.getState().setShellView(view) };
}

/** One compose verb on the open message, disabled when none is open. */
function composeCommand(
  id: string,
  label: string,
  icon: ComponentType<IconProps>,
  verb: (message: MessageRef) => void,
): PaletteCommand {
  const open = useReadingStore.getState().snapshot !== null;
  return {
    id,
    label,
    category: 'Compose',
    icon,
    disabled: open ? undefined : NO_MESSAGE,
    run: () => {
      const message = useReadingStore.getState().snapshot?.message;
      if (message !== undefined) {
        verb(message);
      }
    },
  };
}

/** One mail action on the selected row, disabled when no row stands. */
function rowCommand(
  id: string,
  label: string,
  icon: ComponentType<IconProps>,
  act: (row: MessageRowView) => void,
): PaletteCommand {
  return {
    id,
    label,
    category: 'Mail',
    icon,
    disabled: selectedRow() === undefined ? NO_ROW : undefined,
    run: () => {
      const row = selectedRow();
      if (row !== undefined) {
        act(row);
      }
    },
  };
}

/** One reading-pane position command: the device-local appearance setter. */
function readingCommand(id: string, label: string, position: 'right' | 'bottom' | 'off'): PaletteCommand {
  return {
    id,
    label,
    category: 'View',
    icon: ReadingIcon,
    run: () => useAppearanceStore.getState().setReadingPosition(position),
  };
}

/** The whole command set, rebuilt from the standing stores on every call. */
export function buildCommands(): PaletteCommand[] {
  const row = selectedRow();
  const readLabel = row?.unread === false ? 'Mark unread' : 'Mark read';
  const flagLabel = row?.flagged === true ? 'Unflag' : 'Flag';
  const hasAccount = useAccountStore.getState().accounts.length > 0;

  return [
    viewCommand('go-mail', 'Go to Mail', 'mail', MailIcon),
    viewCommand('go-calendar', 'Go to Calendar', 'calendar', CalendarIcon),
    viewCommand('go-people', 'Go to People', 'people', PeopleIcon),
    folderCommand('go-inbox', 'Go to Inbox', 'inbox', 'Inbox', InboxIcon),
    folderCommand('go-sent', 'Go to Sent', 'sent', 'Sent', SentIcon),
    folderCommand('go-drafts', 'Go to Drafts', 'drafts', 'Drafts', DraftsIcon),
    folderCommand('go-archive', 'Go to Archive', 'archive', 'Archive', ArchiveIcon),
    folderCommand('go-trash', 'Go to Trash', 'trash', 'Trash', TrashIcon),
    {
      id: 'go-unified',
      label: 'Go to Unified Inbox',
      category: 'Go',
      icon: InboxIcon,
      run: () => {
        useFolderStore.getState().selectUnified();
        void dispatch(intents.selectAccount(null));
      },
    },
    {
      id: 'new-email',
      label: 'New Email',
      category: 'Compose',
      icon: ComposeIcon,
      disabled: hasAccount ? undefined : NO_ACCOUNT,
      run: () => {
        if (useAccountStore.getState().accounts.length > 0) {
          void openNewMessageComposer();
        }
      },
    },
    composeCommand('reply', 'Reply', ReplyIcon, (message) => {
      void dispatch(intents.replyTo(message));
    }),
    composeCommand('reply-all', 'Reply all', ReplyAllIcon, (message) => {
      void dispatch(intents.replyAllTo(message));
    }),
    composeCommand('forward', 'Forward', ForwardIcon, (message) => {
      void dispatch(intents.forward(message, carriedAttachmentIds()));
    }),
    rowCommand('toggle-read', readLabel, MarkReadIcon, (target) => {
      void dispatch(intents.markRead(target.key, target.unread));
    }),
    rowCommand('toggle-flag', flagLabel, FlagIcon, (target) => {
      void dispatch(intents.setFlagged(target.key, !target.flagged));
    }),
    rowCommand('archive', 'Archive', ArchiveIcon, (target) => {
      void dispatch(intents.archiveMessage(target.key));
    }),
    rowCommand('delete', 'Delete', TrashIcon, (target) => {
      void dispatch(intents.moveToTrash(target.key));
    }),
    {
      id: 'search-mail',
      label: 'Search mail',
      category: 'Search',
      icon: SearchIcon,
      run: () => {
        openSearchIsland();
      },
    },
    {
      id: 'settings',
      label: 'Settings',
      category: 'App',
      icon: SettingsIcon,
      run: () => useUiStore.getState().setSettingsOpen(true),
    },
    {
      id: 'add-account',
      label: 'Add account',
      category: 'App',
      icon: PlusIcon,
      run: () => useUiStore.getState().setAccountSetupOpen(true),
    },
    {
      id: 'cycle-theme',
      label: 'Cycle theme',
      category: 'App',
      icon: MoonIcon,
      run: () => {
        void dispatch(intents.setTheme(nextThemeMode(useUiStore.getState().themeMode)));
      },
    },
    {
      id: 'toggle-agent',
      label: 'Toggle agent panel',
      category: 'App',
      icon: AgentIcon,
      run: () => {
        const ui = useUiStore.getState();
        ui.setAgentOpen(!ui.agentOpen);
      },
    },
    {
      id: 'toggle-ai',
      label: 'Toggle AI documents panel',
      category: 'App',
      icon: AiIcon,
      run: () => {
        const ui = useUiStore.getState();
        ui.setAiPanelOpen(!ui.aiPanelOpen);
      },
    },
    {
      id: 'sync',
      label: 'Sync mail now',
      category: 'App',
      icon: RefreshIcon,
      run: () => {
        void dispatch(intents.refreshMail());
      },
    },
    {
      id: 'view-flat',
      label: 'Flat view',
      category: 'View',
      icon: BulletListIcon,
      run: () => {
        void dispatch(intents.setViewMode('flat'));
      },
    },
    {
      id: 'view-threaded',
      label: 'Threaded view',
      category: 'View',
      icon: OrderedListIcon,
      run: () => {
        void dispatch(intents.setViewMode('threaded'));
      },
    },
    readingCommand('reading-right', 'Reading pane right', 'right'),
    readingCommand('reading-bottom', 'Reading pane bottom', 'bottom'),
    readingCommand('reading-off', 'Reading pane off', 'off'),
  ];
}
