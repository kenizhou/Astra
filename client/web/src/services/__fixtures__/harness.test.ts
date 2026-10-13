// SPDX-License-Identifier: GPL-3.0-only
// The fixture harness's contract: every pull the shell's hydration makes
// resolves with the snapshot the surface renders, mutations settle quietly,
// and no event ever fires, so a plain browser shows the real UI at rest.
import { describe, expect, it, vi } from 'vitest';

import calendarFixture from './calendar_snapshot.json';
import { invoke, listen } from './harness';
import mailboxListFixture from './mailbox_list.json';
import mcpStatusFixture from './mcp_status.json';
import peopleFixture from './people_snapshot.json';
import settingsFixture from './settings_snapshot.json';

describe('fixture harness', () => {
  it('marks the window as Tauri so the hydration path runs', () => {
    expect('__TAURI_INTERNALS__' in window).toBe(true);
  });

  it('serves the recorded snapshots for the hydrated surfaces', async () => {
    await expect(invoke('mailbox_list')).resolves.toEqual(mailboxListFixture);
    await expect(invoke('calendar')).resolves.toEqual(calendarFixture);
    await expect(invoke('people')).resolves.toEqual(peopleFixture);
    await expect(invoke('settings')).resolves.toEqual(settingsFixture);
    await expect(invoke('mcp_status')).resolves.toEqual(mcpStatusFixture);
  });

  it('hands a fresh copy on every pull, so a surface mutating its snapshot never poisons the next pull', async () => {
    const first = (await invoke('mailbox_list')) as { account_folders: unknown[] };
    first.account_folders.length = 0;
    const second = (await invoke('mailbox_list')) as { account_folders: unknown[] };
    expect(second.account_folders).toHaveLength(mailboxListFixture.account_folders.length);
  });

  it('answers the surfaces at rest with their empty shapes', async () => {
    await expect(invoke('sync_progress')).resolves.toEqual({ accounts: [] });
    await expect(invoke('sending')).resolves.toEqual({ rows: [] });
    await expect(invoke('split_map', { account: 'work' })).resolves.toEqual([]);
    await expect(invoke('agent_status')).resolves.toEqual({ state: 'idle', model: null });
  });

  it('answers the nullable pulls with null', async () => {
    await expect(invoke('reading')).resolves.toBeNull();
    await expect(invoke('draft_status')).resolves.toBeNull();
    await expect(invoke('compose_request')).resolves.toBeNull();
  });

  it('settles an unrecorded command as null and says so once per command', async () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => undefined);
    await expect(invoke('dispatch', { intent: 'refresh_mail' })).resolves.toBeNull();
    await expect(invoke('dispatch', { intent: 'refresh_mail' })).resolves.toBeNull();
    const mentions = info.mock.calls.filter((args) => String(args[0]).includes('dispatch'));
    expect(mentions).toHaveLength(1);
    info.mockRestore();
  });

  it('registers listeners as inert: an unlisten comes back and no event fires', async () => {
    const handler = vi.fn();
    const unlisten = await listen('surface_changed', handler);
    expect(typeof unlisten).toBe('function');
    expect(() => unlisten()).not.toThrow();
    expect(handler).not.toHaveBeenCalled();
  });
});
