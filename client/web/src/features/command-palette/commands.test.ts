// SPDX-License-Identifier: GPL-3.0-only
// The palette registry: every command maps to an action the shell already
// ships, resolves its target at call time, and stands disabled with a reason
// when the target is absent. The intents cross through the mocked ipc; the
// local setters move their stores; send never appears.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

const composer = vi.hoisted(() => ({ openNewMessageComposer: vi.fn() }));

vi.mock('@/features/composer/composeWindow', () => ({
  openNewMessageComposer: composer.openNewMessageComposer,
}));

import { registerSearchIslandOpener } from '@/components/layout/searchIsland';
import { useAttachmentBrowserStore } from '@/features/attachment-browser/attachmentBrowserStore';
import { usePinnedSearchesStore } from '@/features/pinned-searches/pinnedSearchesStore';
import { useAppearanceStore } from '@/services/appearance';
import type { AccountFolderRow, FolderRow, MessageRowView } from '@/services/surfaces';
import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { useUiStore } from '@/stores/uiStore';

import { buildCommands, type PaletteCommand } from './commands';

/** One folder row of a given role, overridable per test. */
function folder(role: FolderRow['role'], key: string): FolderRow {
  return {
    key,
    name: key,
    role,
    unread: 0,
    depth: 0,
    has_children: false,
    expanded: true,
  };
}

/** One account group holding the five role folders the Go commands name. */
function account(id: string, roles: FolderRow['role'][] = ['inbox', 'sent', 'drafts', 'archive', 'trash']): AccountFolderRow {
  return {
    account_id: id,
    display_name: id,
    address: id + '@example.test',
    expanded: true,
    folders: roles.map((role) => folder(role, `${id}:${role ?? 'other'}`)),
  };
}

/** One mailbox-list row, overridable per test. */
function row(over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key: { message: { account: 'work', key: 'imap:v1:u1' } },
    account_id: 'work',
    sender_name: 'Ada Lovelace',
    sender_address: 'ada@example.test',
    subject: 'Launch',
    sent_at: '2026-10-09T08:30:00Z',
    unread: true,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

/** The command with this id; the registry always carries it. */
function command(id: string): PaletteCommand {
  const found = buildCommands().find((entry) => entry.id === id);
  if (found === undefined) {
    throw new Error(`no command ${id}`);
  }
  return found;
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  composer.openNewMessageComposer.mockReset();
  composer.openNewMessageComposer.mockResolvedValue(undefined);
  useUiStore.setState({
    shellView: 'mail',
    themeMode: 'follow_host',
    agentOpen: false,
    aiPanelOpen: false,
    settingsOpen: false,
    accountSetupOpen: false,
  });
  useAccountStore.setState({ accounts: [] });
  useFolderStore.setState({ accounts: [], selected: null, outboxAccount: null, unified: false, scope: null });
  useListStore.setState({ rows: [], selectedKey: null, selectedKeys: [] });
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useAppearanceStore.setState({ readingPosition: 'right' });
  usePinnedSearchesStore.setState({ searches: [], panelOpen: false });
  useAttachmentBrowserStore.setState({ isOpen: false, sender: null });
  registerSearchIslandOpener(null);
});

afterEach(() => {
  registerSearchIslandOpener(null);
});

describe('the Go commands', () => {
  it('switches the shell view without an intent', () => {
    command('go-calendar').run();
    expect(useUiStore.getState().shellView).toBe('calendar');
    command('go-people').run();
    expect(useUiStore.getState().shellView).toBe('people');
    command('go-mail').run();
    expect(useUiStore.getState().shellView).toBe('mail');
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('selects the role folder of the first account and dispatches selectFolder', () => {
    useFolderStore.setState({ accounts: [account('work')] });
    command('go-inbox').run();
    const ref = { account: 'work', key: 'work:inbox' };
    expect(useFolderStore.getState().selected).toEqual(ref);
    expect(ipc.dispatch).toHaveBeenCalledWith({ select_folder: { folder: ref } });
  });

  it('reads the standing account from the scope before the first account', () => {
    useFolderStore.setState({
      accounts: [account('work'), account('home')],
      scope: { kind: 'account', account: 'home' },
    });
    command('go-sent').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({
      select_folder: { folder: { account: 'home', key: 'home:sent' } },
    });
  });

  it('reads the standing account from the selected folder', () => {
    useFolderStore.setState({
      accounts: [account('work'), account('home')],
      selected: { account: 'home', key: 'home:inbox' },
    });
    command('go-drafts').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({
      select_folder: { folder: { account: 'home', key: 'home:drafts' } },
    });
  });

  it('stands disabled with a reason when no account stands', () => {
    const inbox = command('go-inbox');
    expect(inbox.disabled).toBe('Add an account first');
    inbox.run();
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('stands disabled with a reason when the account lacks the role folder', () => {
    useFolderStore.setState({ accounts: [account('work', ['inbox'])] });
    expect(command('go-inbox').disabled).toBeUndefined();
    const archive = command('go-archive');
    expect(archive.disabled).toBe('This account has no Archive folder');
    archive.run();
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('selects the unified inbox through selectAccount null', () => {
    command('go-unified').run();
    expect(useFolderStore.getState().unified).toBe(true);
    expect(ipc.dispatch).toHaveBeenCalledWith({ select_account: null });
  });
});

describe('the Compose commands', () => {
  it('New Email opens the composer when an account stands', () => {
    useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work', address: 'me@work.example' }] });
    const fresh = command('new-email');
    expect(fresh.disabled).toBeUndefined();
    fresh.run();
    expect(composer.openNewMessageComposer).toHaveBeenCalledTimes(1);
  });

  it('New Email stands disabled with a reason without an account', () => {
    const fresh = command('new-email');
    expect(fresh.disabled).toBe('Add an account first');
    fresh.run();
    expect(composer.openNewMessageComposer).not.toHaveBeenCalled();
  });

  it('Reply, Reply all, and Forward dispatch the compose family for the open message', () => {
    useReadingStore.setState({
      snapshot: {
        message: { account: 'work', key: 'imap:v1:u1' },
        subject: 'Launch',
        from_name: 'Ada',
        from_address: 'ada@example.test',
        sent_at: '2026-10-09T08:30:00Z',
        remote_image_count: 0,
        attachments: [{ id: 7, file_name: 'spec.pdf', media_type: 'application/pdf', size: 10 }],
      },
    });
    const message = { account: 'work', key: 'imap:v1:u1' };
    command('reply').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ compose: { reply: { message } } });
    command('reply-all').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ compose: { reply_all: { message } } });
    command('forward').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ compose: { forward: { message, carry: [7] } } });
  });

  it('the compose verbs stand disabled with a reason when no message is open', () => {
    for (const id of ['reply', 'reply-all', 'forward']) {
      const entry = command(id);
      expect(entry.disabled).toBe('Open a message first');
      entry.run();
    }
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });
});

describe('the Mail commands', () => {
  it('act on the selected row through the ribbon intents', () => {
    const selected = row();
    useListStore.setState({ rows: [selected], selectedKey: selected.key });
    command('toggle-read').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ mark_read: { row: selected.key, read: true } });
    command('toggle-flag').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ set_flagged: { row: selected.key, flagged: true } });
    command('archive').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ archive_message: { row: selected.key } });
    command('delete').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ move_to_trash: { row: selected.key } });
  });

  it('name the state the read and flag toggles leave behind', () => {
    const unread = row({ unread: true, flagged: false });
    useListStore.setState({ rows: [unread], selectedKey: unread.key });
    expect(command('toggle-read').label).toBe('Mark read');
    expect(command('toggle-flag').label).toBe('Flag');

    const read = row({ unread: false, flagged: true });
    useListStore.setState({ rows: [read], selectedKey: read.key });
    expect(command('toggle-read').label).toBe('Mark unread');
    expect(command('toggle-flag').label).toBe('Unflag');
    command('toggle-read').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ mark_read: { row: read.key, read: false } });
    command('toggle-flag').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ set_flagged: { row: read.key, flagged: false } });
  });

  it('stand disabled with a reason when no row is selected', () => {
    useListStore.setState({ rows: [row()], selectedKey: null });
    for (const id of ['toggle-read', 'toggle-flag', 'archive', 'delete']) {
      const entry = command(id);
      expect(entry.disabled).toBe('Select a message first');
      entry.run();
    }
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('stand disabled when the selected key names no standing row', () => {
    useListStore.setState({ rows: [], selectedKey: { message: { account: 'work', key: 'gone' } } });
    expect(command('archive').disabled).toBe('Select a message first');
  });
});

describe('the Search, App, and View commands', () => {
  it('Search mail opens the island through the seam', () => {
    const open = vi.fn();
    registerSearchIslandOpener(open);
    command('search-mail').run();
    expect(open).toHaveBeenCalledTimes(1);
  });

  it('Saved searches opens the panel through the store gate', () => {
    command('saved-searches').run();
    expect(usePinnedSearchesStore.getState().panelOpen).toBe(true);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('Browse attachments opens the browser through the store gate', () => {
    command('attachment-browser').run();
    expect(useAttachmentBrowserStore.getState().isOpen).toBe(true);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('Settings and Add account open their surfaces', () => {
    command('settings').run();
    expect(useUiStore.getState().settingsOpen).toBe(true);
    command('add-account').run();
    expect(useUiStore.getState().accountSetupOpen).toBe(true);
  });

  it('Cycle theme persists through the settings intent, never the local setter', () => {
    useUiStore.setState({ themeMode: 'light' });
    command('cycle-theme').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ settings: { set_theme: { mode: 'dark' } } });
    expect(useUiStore.getState().themeMode).toBe('light');
  });

  it('the panel toggles flip their own flags', () => {
    command('toggle-agent').run();
    expect(useUiStore.getState().agentOpen).toBe(true);
    command('toggle-agent').run();
    expect(useUiStore.getState().agentOpen).toBe(false);
    command('toggle-ai').run();
    expect(useUiStore.getState().aiPanelOpen).toBe(true);
  });

  it('Sync mail now dispatches refreshMail', () => {
    command('sync').run();
    expect(ipc.dispatch).toHaveBeenCalledWith('refresh_mail');
  });

  it('the view modes dispatch setViewMode', () => {
    command('view-flat').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ set_view_mode: 'flat' });
    command('view-threaded').run();
    expect(ipc.dispatch).toHaveBeenCalledWith({ set_view_mode: 'threaded' });
  });

  it('the reading pane positions move the device-local appearance store', () => {
    command('reading-bottom').run();
    expect(useAppearanceStore.getState().readingPosition).toBe('bottom');
    command('reading-off').run();
    expect(useAppearanceStore.getState().readingPosition).toBe('off');
    command('reading-right').run();
    expect(useAppearanceStore.getState().readingPosition).toBe('right');
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });
});

describe('the registry discipline', () => {
  it('ships no send command and none of the deferred verbs', () => {
    const all = buildCommands();
    expect(all.some((entry) => /send/i.test(entry.label))).toBe(false);
    expect(all.some((entry) => /spam|permanently|undo|outbox/i.test(entry.label))).toBe(false);
  });

  it('carries a unique id, a label, a category, and an icon for every command', () => {
    const all = buildCommands();
    const ids = all.map((entry) => entry.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const entry of all) {
      expect(entry.label.length).toBeGreaterThan(0);
      expect(entry.category.length).toBeGreaterThan(0);
      expect(entry.icon).toBeTruthy();
      expect(typeof entry.run).toBe('function');
    }
  });
});
